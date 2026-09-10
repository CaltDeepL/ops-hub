//! `POST /v1/runs`
//!
//! リクエストボディは無い。ハンドラの仕事は
//! [`RunOutcome`](crate::service::run_service::RunOutcome) をステータスコードに
//! 写すことだけで、判断は service 側にある。
//!
//! | 状況 | ステータス | ボディ |
//! |---|---|---|
//! | ロック取得成功 | 202 | `{"run_id": "...", "status": "started"}` |
//! | ロック競合 | 200 | `{"run_id": "..." または null, "status": "already_running"}` |
//! | DB接続不可 | 503 | problem+json（`AppError::Database` 経由） |
//!
//! `recovered: true`（取り残しロックを回収して起動した場合）はタスク7で足す。
//! レート制限（429）も同様に未実装。

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use uuid::Uuid;

use crate::error::AppResult;
use crate::service::run_service::{self, RunOutcome};
use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct RunAcceptedResponse {
    /// 競合時に実行中の run を引けなかった場合のみ `null`。
    pub run_id: Option<Uuid>,
    pub status: &'static str,
}

pub async fn start_run(State(state): State<AppState>) -> AppResult<Response> {
    let response = match run_service::start(&state).await? {
        RunOutcome::Started { run_id } => (
            StatusCode::ACCEPTED,
            Json(RunAcceptedResponse {
                run_id: Some(run_id),
                status: "started",
            }),
        ),
        // 競合は 200。409 にしない（D-3）
        RunOutcome::AlreadyRunning { run_id } => (
            StatusCode::OK,
            Json(RunAcceptedResponse {
                run_id,
                status: "already_running",
            }),
        ),
    };

    Ok(response.into_response())
}
