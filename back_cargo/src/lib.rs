pub mod config;
pub mod error;
pub mod handler;
pub mod run_lock;
pub mod state;
pub mod trace_id;

use axum::{Router, routing::get};
use tower_http::trace::TraceLayer;

use crate::state::AppState;

/// ルータを組み立てる。
///
/// 統合テストからも同じ関数を呼べるように、`main.rs` ではなく `lib.rs` に置く
/// （基本設計 6章）。
pub fn app(state: AppState) -> Router {
    Router::new()
        .route("/health", get(handler::health::health))
        .route("/livez", get(handler::livez::livez))
        .layer(TraceLayer::new_for_http())
        // 後に足したレイヤほど外側。trace_id を最外に置くことで、
        // TraceLayer が出すログにもスパンの trace_id が載る
        .layer(axum::middleware::from_fn(trace_id::propagate))
        .with_state(state)
}
