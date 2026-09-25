//! ハンドラ間で共有する状態。
//!
//! `PgPool` は内部が `Arc` なので `clone()` は安い。`AppState` 自体も
//! Axum の `State` 抽出のために `Clone` である必要がある。
//! `reqwest::Client` も内部が `Arc` で、コネクションプールを共有するために
//! 1つを使い回す（リクエストごとに作ると TLS ハンドシェイクを毎回やり直す）。
use crate::config::Config;
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub config: Arc<Config>,
    /// probe 用。[`build_client`](crate::provider::probe::build_client) で作る。
    pub http: reqwest::Client,
}
