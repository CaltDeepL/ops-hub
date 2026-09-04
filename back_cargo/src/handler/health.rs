//! `GET /health`
//!
//! DBに到達できるかどうかまで見る。到達できなければ 503 を返す。
//! プールは `connect_lazy` なので、プロセスは起動できてもDBが落ちている状態が
//! ありうる。その差をここで表面化させる。

use std::time::Duration;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde::Serialize;

use crate::state::AppState;

/// DBへの疎通確認そのものに掛ける上限。
///
/// プールの `acquire_timeout` とは別に張る。接続取得後にサーバが応答しない
/// ケースで `/health` が固まると、コンテナの HEALTHCHECK 側がタイムアウトして
/// 原因が分からなくなるため。
const DB_PROBE_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Serialize)]
pub struct HealthBody {
    status: &'static str,
    version: &'static str,
    db: &'static str,
}

pub async fn health(State(state): State<AppState>) -> impl IntoResponse {
    let probe = tokio::time::timeout(
        DB_PROBE_TIMEOUT,
        sqlx::query_scalar::<_, i32>("SELECT 1").fetch_one(&state.db),
    )
    .await;

    let db_ok = match probe {
        Ok(Ok(_)) => true,
        Ok(Err(e)) => {
            tracing::warn!(error = %e, "health: DBへの疎通に失敗しました");
            false
        }
        Err(_) => {
            tracing::warn!(
                timeout_ms = DB_PROBE_TIMEOUT.as_millis() as u64,
                "health: DBへの疎通がタイムアウトしました"
            );
            false
        }
    };

    let status = if db_ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (
        status,
        Json(HealthBody {
            status: if db_ok { "ok" } else { "degraded" },
            version: env!("CARGO_PKG_VERSION"),
            db: if db_ok { "up" } else { "down" },
        }),
    )
}