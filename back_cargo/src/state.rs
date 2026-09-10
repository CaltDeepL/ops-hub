//! ハンドラ間で共有する状態。
//!
//! `PgPool` は内部が `Arc` なので `clone()` は安い。`AppState` 自体も
//! Axum の `State` 抽出のために `Clone` である必要がある。

use std::sync::Arc;

use sqlx::PgPool;

use crate::config::Config;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    /// 設定。`AppState` はリクエストごとに clone されるため、`Config` を直に
    /// 持つと `database_url` の `String` を毎回複製することになる。`Arc` で包む。
    ///
    /// タスク6では `run_lock_key` のためだけに必要だが、以降のタスク
    /// （通知トークン・Slack Webhook URL・スイーパーの閾値）でも同じ経路を使う。
    pub config: Arc<Config>,
}
