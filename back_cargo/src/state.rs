//! ハンドラ間で共有する状態。
//!
//! `PgPool` は内部が `Arc` なので `clone()` は安い。`AppState` 自体も
//! Axum の `State` 抽出のために `Clone` である必要がある。

use sqlx::PgPool;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
}
