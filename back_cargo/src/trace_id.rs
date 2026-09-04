//! リクエストごとの `trace_id`（N-12）。
//!
//! `AppError::into_response` は `IntoResponse` の中で呼ばれるため、
//! リクエストの extension には触れない。そこで task-local に置き、
//! ハンドラの引数を増やさずにレスポンス生成時点で読めるようにする。

use axum::{
    extract::Request,
    http::{HeaderName, HeaderValue},
    middleware::Next,
    response::Response,
};
use tracing::Instrument as _;
use uuid::Uuid;

/// 受け取り／返却に使うヘッダ名。
pub const HEADER: HeaderName = HeaderName::from_static("x-request-id");

tokio::task_local! {
    static TRACE_ID: String;
}

/// 現在のリクエストの `trace_id`。スコープ外（バッチ処理の外側など）では `"-"`。
pub fn current() -> String {
    TRACE_ID
        .try_with(|id| id.clone())
        .unwrap_or_else(|_| "-".to_owned())
}

/// 実行（`POST /v1/runs` のバックグラウンド処理）にも同じ仕組みを使うためのヘルパ。
/// 1実行を1つのIDで追える（N-12）。
pub async fn with_new_trace_id<F, T>(future: F) -> T
where
    F: Future<Output = T>,
{
    let trace_id = Uuid::new_v4().to_string();
    let span = tracing::info_span!("run", trace_id = %trace_id);
    TRACE_ID.scope(trace_id, future.instrument(span)).await
}

/// `axum::middleware::from_fn` に渡すミドルウェア。
///
/// - 受信ヘッダに妥当な `x-request-id` があれば引き継ぎ、無ければ採番する
/// - `tracing` のスパンに載せるので、構造化ログの全行に `trace_id` が付く
/// - 同じ値をレスポンスヘッダにも返す
pub async fn propagate(request: Request, next: Next) -> Response {
    let trace_id = request
        .headers()
        .get(&HEADER)
        .and_then(|value| value.to_str().ok())
        .filter(|value| is_safe(value))
        .map(str::to_owned)
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    let span = tracing::info_span!("request", trace_id = %trace_id);
    let header_value = HeaderValue::from_str(&trace_id).ok();

    let mut response = TRACE_ID
        .scope(trace_id, next.run(request).instrument(span))
        .await;

    if let Some(value) = header_value {
        response.headers_mut().insert(HEADER, value);
    }
    response
}

/// 外部から来た値をそのままログに載せるため、文字種と長さを絞る。
/// 改行や制御文字を通すと構造化ログを壊される（ログインジェクション）。
fn is_safe(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_uuid_like_values() {
        assert!(is_safe("0f9c1d2e-3b4a-5c6d-7e8f-90a1b2c3d4e5"));
        assert!(is_safe("run_12345"));
    }

    #[test]
    fn rejects_values_that_could_break_logs() {
        assert!(!is_safe(""));
        assert!(!is_safe("abc\ndef"));
        assert!(!is_safe("a b"));
        assert!(!is_safe(&"x".repeat(65)));
    }

    #[tokio::test]
    async fn current_returns_placeholder_outside_scope() {
        assert_eq!(current(), "-");
    }

    #[tokio::test]
    async fn current_returns_the_scoped_value() {
        let id = TRACE_ID
            .scope("fixed-id".to_owned(), async { current() })
            .await;
        assert_eq!(id, "fixed-id");
    }
}