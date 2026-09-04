//! ops-hub（監視・通知ハブ）
//!
//! ルータの組み立てをライブラリ側に置いているのは、統合テスト（`tests/`）から
//! `app(state)` を直接呼べるようにするため。タスク6以降のテストがここに乗る。

pub mod config;
pub mod handler;
pub mod state;

use axum::Router;
use axum::routing::get;
use tower_http::trace::TraceLayer;

use crate::state::AppState;

pub fn app(state: AppState) -> Router {
    Router::new()
        .route("/health", get(handler::health::health))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}