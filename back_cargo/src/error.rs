//! アプリケーションのエラー型と RFC 9457（problem+json）への変換。
//!
//! 方針は3つ。
//!
//! 1. **5xx は `trace_id` しか返さない**（N-10）。内部の事情はログに残す
//! 2. **SQLSTATE の分類は純粋関数に切り出す**。`sqlx::Error` は外から組み立てられないため、
//!    `(SQLSTATE, 制約名)` を引数に取る関数にしておかないとユニットテストが書けない
//! 3. **ロック競合はここに入れない**。正常系なので成功型で表す（詳細設計 3.2）

use axum::{
    Json,
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Serialize;

use crate::trace_id;

// =============================================================================
// AppError
// =============================================================================

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}が見つかりません")]
    NotFound(&'static str),
    #[error("{0}")]
    BadRequest(String),
    #[error("認証が必要です")]
    Unauthorized,
    #[error("{0}")]
    Unprocessable(String),
    #[error("リクエストが多すぎます")]
    TooManyRequests,
    #[error("データベースエラー")]
    Database(#[source] sqlx::Error),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

pub type AppResult<T> = Result<T, AppError>;

impl AppError {
    pub fn status(&self) -> StatusCode {
        match self {
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::Unprocessable(_) => StatusCode::UNPROCESSABLE_ENTITY,
            Self::TooManyRequests => StatusCode::TOO_MANY_REQUESTS,
            // DB由来のうち「繋がらない」は 503（詳細設計 2.2）。それ以外は 500
            Self::Database(error) if is_connectivity_error(error) => {
                StatusCode::SERVICE_UNAVAILABLE
            }
            Self::Database(_) | Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// RFC 9457 の `type`。個別のURIは立てず、機械可読な識別子だけ与える。
    fn problem_type(&self) -> &'static str {
        match self {
            Self::NotFound(_) => "urn:ops-hub:error:not-found",
            Self::BadRequest(_) => "urn:ops-hub:error:bad-request",
            Self::Unauthorized => "urn:ops-hub:error:unauthorized",
            Self::Unprocessable(_) => "urn:ops-hub:error:unprocessable",
            Self::TooManyRequests => "urn:ops-hub:error:too-many-requests",
            Self::Database(_) | Self::Internal(_) => "urn:ops-hub:error:internal",
        }
    }

    fn title(&self) -> &'static str {
        match self.status() {
            StatusCode::NOT_FOUND => "見つかりません",
            StatusCode::BAD_REQUEST => "リクエストが不正です",
            StatusCode::UNAUTHORIZED => "認証が必要です",
            StatusCode::UNPROCESSABLE_ENTITY => "入力値を処理できません",
            StatusCode::TOO_MANY_REQUESTS => "リクエストが多すぎます",
            StatusCode::SERVICE_UNAVAILABLE => "一時的に利用できません",
            _ => "サーバ内部エラー",
        }
    }
}

// =============================================================================
// problem+json
// =============================================================================

#[derive(Debug, Serialize)]
pub struct ProblemDetails {
    #[serde(rename = "type")]
    pub problem_type: &'static str,
    pub title: &'static str,
    pub status: u16,
    /// 4xx のみ。5xx では常に `None`（N-10）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub trace_id: String,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status();
        let trace_id = trace_id::current();

        // 詳細はログにだけ残す。`?self` で source チェーンごと出力する
        if status.is_server_error() {
            tracing::error!(trace_id = %trace_id, status = status.as_u16(), error = ?self, "サーバエラー");
        } else {
            tracing::warn!(trace_id = %trace_id, status = status.as_u16(), error = %self, "クライアントエラー");
        }

        let body = ProblemDetails {
            problem_type: self.problem_type(),
            title: self.title(),
            status: status.as_u16(),
            detail: if status.is_server_error() {
                None
            } else {
                Some(self.to_string())
            },
            trace_id,
        };

        let mut response = (status, Json(body)).into_response();
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/problem+json"),
        );
        if status == StatusCode::UNAUTHORIZED {
            // 詳細設計 2.3：401 には WWW-Authenticate を付ける
            response
                .headers_mut()
                .insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
        }
        response
    }
}

// =============================================================================
// SQLSTATE の分類（詳細設計 3.3）
// =============================================================================

/// 制約違反の扱い。`sqlx::Error` を作らずにテストできるよう、分類だけを型にする。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbErrorClass {
    /// 期待どおりの重複。**呼び出し側（repository）で握りつぶし、正常応答にする**。
    /// ここまで上がってきたら実装漏れなので 500 にする
    ExpectedDuplicate,
    /// ロジックの不整合。500 にしてログへ警告を残す
    LogicViolation,
    /// 入力が制約に反している。422
    Unprocessable,
    /// DBに繋がらない。503
    Unavailable,
    /// 分類できないもの。500
    Unexpected,
}

/// `(SQLSTATE, 制約名)` から分類する。
///
/// 対応表は詳細設計 3.3。制約名で分岐するため、マイグレーションの制約名を変えたら
/// ここも変える。名前はタスク02・03の検証スクリプトで実際に観測した値。
pub fn classify(sqlstate: &str, constraint: Option<&str>) -> DbErrorClass {
    match (sqlstate, constraint) {
        // 23505：一意制約違反。3通りに分かれるのが本プロジェクト特有
        ("23505", Some("events_source_idempotency_key")) => DbErrorClass::ExpectedDuplicate,
        ("23505", Some("outbox_dedupe_key")) => DbErrorClass::ExpectedDuplicate,
        ("23505", Some("incidents_one_open_per_target")) => DbErrorClass::LogicViolation,
        ("23505", _) => DbErrorClass::Unprocessable,

        ("23514", _) => DbErrorClass::Unprocessable, // CHECK制約
        ("23503", _) => DbErrorClass::Unprocessable, // 外部キー
        ("23502", _) => DbErrorClass::Unprocessable, // NOT NULL
        ("22P02", _) => DbErrorClass::Unprocessable, // ENUM等への不正な入力値

        // Class 08：接続系（08000 / 08003 / 08006 …）
        (code, _) if code.starts_with("08") => DbErrorClass::Unavailable,
        // Class 57：管理者による切断（57P01 = admin_shutdown、脱出ハッチで起こりうる）
        (code, _) if code.starts_with("57") => DbErrorClass::Unavailable,

        _ => DbErrorClass::Unexpected,
    }
}

/// SQLSTATE を持たない `sqlx::Error`（プール枯渇・I/O・TLS）も「繋がらない」に含める。
fn is_connectivity_error(error: &sqlx::Error) -> bool {
    if matches!(
        error,
        sqlx::Error::PoolTimedOut
            | sqlx::Error::PoolClosed
            | sqlx::Error::Io(_)
            | sqlx::Error::Tls(_)
    ) {
        return true;
    }
    db_class(error) == Some(DbErrorClass::Unavailable)
}

/// `sqlx::Error` から `(SQLSTATE, 制約名)` を取り出して分類する。
pub fn db_class(error: &sqlx::Error) -> Option<DbErrorClass> {
    let db_error = error.as_database_error()?;
    let code = db_error.code()?;
    Some(classify(code.as_ref(), db_error.constraint()))
}

impl From<sqlx::Error> for AppError {
    fn from(error: sqlx::Error) -> Self {
        if matches!(error, sqlx::Error::RowNotFound) {
            return Self::NotFound("リソース");
        }

        match db_class(&error) {
            Some(DbErrorClass::Unprocessable) => {
                let constraint = error
                    .as_database_error()
                    .and_then(|e| e.constraint())
                    .unwrap_or("unknown");
                Self::Unprocessable(format!("入力値が制約に反しています（{constraint}）"))
            }
            Some(DbErrorClass::ExpectedDuplicate) => {
                // repository 層で握りつぶすはずのものが漏れてきた
                tracing::warn!(error = ?error, "正常系として扱うべき重複がハンドラまで到達した");
                Self::Database(error)
            }
            Some(DbErrorClass::LogicViolation) => {
                tracing::warn!(error = ?error, "ロジックの不整合を検出（未解決インシデントの二重生成など）");
                Self::Database(error)
            }
            _ => Self::Database(error),
        }
    }
}

/// 制約名で分岐するための拡張トレイト（既存プロジェクトの `OnConstraint` 相当）。
///
/// ```ignore
/// let result = sqlx::query!(...).execute(&mut *tx).await;
/// if result.is_unique_violation_on("outbox_dedupe_key") {
///     return Ok(Skipped);   // D-4：多重起動で期待どおり弾かれた。正常系
/// }
/// ```
pub trait OnConstraint {
    fn constraint_name(&self) -> Option<&str>;
    fn is_unique_violation_on(&self, constraint: &str) -> bool;
}

impl OnConstraint for sqlx::Error {
    fn constraint_name(&self) -> Option<&str> {
        self.as_database_error()?.constraint()
    }

    fn is_unique_violation_on(&self, constraint: &str) -> bool {
        let Some(db_error) = self.as_database_error() else {
            return false;
        };
        db_error.code().is_some_and(|code| code == "23505")
            && db_error.constraint() == Some(constraint)
    }
}

impl<T> OnConstraint for Result<T, sqlx::Error> {
    fn constraint_name(&self) -> Option<&str> {
        self.as_ref().err()?.constraint_name()
    }

    fn is_unique_violation_on(&self, constraint: &str) -> bool {
        self.as_ref()
            .err()
            .is_some_and(|error| error.is_unique_violation_on(constraint))
    }
}

// =============================================================================
// tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ---- SQLSTATE の分類（タスク02・03で実測した制約名を使う）----

    #[test]
    fn idempotency_key_duplicate_is_expected() {
        assert_eq!(
            classify("23505", Some("events_source_idempotency_key")),
            DbErrorClass::ExpectedDuplicate
        );
    }

    #[test]
    fn dedupe_key_duplicate_is_expected() {
        assert_eq!(
            classify("23505", Some("outbox_dedupe_key")),
            DbErrorClass::ExpectedDuplicate
        );
    }

    #[test]
    fn double_open_incident_is_a_logic_violation() {
        assert_eq!(
            classify("23505", Some("incidents_one_open_per_target")),
            DbErrorClass::LogicViolation
        );
    }

    #[test]
    fn other_unique_violations_are_unprocessable() {
        assert_eq!(
            classify("23505", Some("targets_name_key")),
            DbErrorClass::Unprocessable
        );
    }

    #[test]
    fn constraint_violations_are_unprocessable() {
        for code in ["23514", "23503", "23502", "22P02"] {
            assert_eq!(
                classify(code, Some("targets_url_https")),
                DbErrorClass::Unprocessable,
                "SQLSTATE {code}"
            );
        }
    }

    #[test]
    fn connection_classes_are_unavailable() {
        assert_eq!(classify("08006", None), DbErrorClass::Unavailable);
        assert_eq!(classify("57P01", None), DbErrorClass::Unavailable);
    }

    #[test]
    fn unknown_codes_fall_through() {
        assert_eq!(classify("42P01", None), DbErrorClass::Unexpected);
    }

    // ---- ステータスコードの対応 ----

    #[test]
    fn status_codes_match_the_api_contract() {
        assert_eq!(AppError::NotFound("対象").status(), StatusCode::NOT_FOUND);
        assert_eq!(AppError::Unauthorized.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            AppError::Unprocessable("だめ".into()).status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            AppError::TooManyRequests.status(),
            StatusCode::TOO_MANY_REQUESTS
        );
        assert_eq!(
            AppError::Database(sqlx::Error::PoolTimedOut).status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
        assert_eq!(
            AppError::Internal(anyhow::anyhow!("boom")).status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test]
    fn row_not_found_becomes_404() {
        let error = AppError::from(sqlx::Error::RowNotFound);
        assert_eq!(error.status(), StatusCode::NOT_FOUND);
    }

    // ---- レスポンスの中身（N-10）----

    async fn body_of(error: AppError) -> (StatusCode, serde_json::Value, String) {
        let response = error.into_response();
        let status = response.status();
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_owned();
        let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
            .await
            .expect("ボディを読み出せる");
        let json = serde_json::from_slice(&bytes).expect("problem+json として解釈できる");
        (status, json, content_type)
    }

    #[tokio::test]
    async fn server_errors_expose_only_trace_id() {
        let secret = "https://hooks.slack.com/services/T000/B000/xxxxxxxx";
        let (status, json, content_type) = body_of(AppError::Internal(anyhow::anyhow!(
            "Slack投稿に失敗: {secret}"
        )))
        .await;

        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(content_type, "application/problem+json");
        assert!(json.get("detail").is_none(), "5xx に detail を出さない");
        assert!(json["trace_id"].is_string());
        assert!(
            !json.to_string().contains("hooks.slack.com"),
            "内部の詳細がレスポンスに漏れている"
        );
    }

    #[tokio::test]
    async fn client_errors_keep_the_detail() {
        let (status, json, _) = body_of(AppError::Unprocessable(
            "source がトークンと一致しません".into(),
        ))
        .await;

        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(json["detail"], "source がトークンと一致しません");
        assert_eq!(json["status"], 422);
        assert_eq!(json["type"], "urn:ops-hub:error:unprocessable");
    }

    #[tokio::test]
    async fn unauthorized_carries_www_authenticate() {
        let response = AppError::Unauthorized.into_response();
        assert_eq!(
            response
                .headers()
                .get(header::WWW_AUTHENTICATE)
                .and_then(|v| v.to_str().ok()),
            Some("Bearer")
        );
    }
}
