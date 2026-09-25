//! `targets` の読み出し。
//!
//! 監視対象の登録 API（F-1）は未実装。現状は `psql` で直接入れる運用。

use sqlx::PgPool;
use uuid::Uuid;

/// probe に必要な列だけを持つ監視対象。
///
/// `severity` は通知（タスク11以降）まで使わないので、まだ読まない。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub id: Uuid,
    pub name: String,
    pub url: String,
    pub method: String,
    pub expected_status: i32,
    pub timeout_ms: i32,
    pub degraded_threshold_ms: i32,
}

/// 有効な監視対象を名前順で返す。
///
/// 名前順にしているのは、ログと `checks` の並びを実行ごとに安定させるため。
/// probe は並行に走るので記録順は保証しないが、投入順は揃う。
pub async fn list_enabled(db: &PgPool) -> sqlx::Result<Vec<Target>> {
    sqlx::query_as!(
        Target,
        r#"
        SELECT id,
               name,
               url,
               method,
               expected_status,
               timeout_ms,
               degraded_threshold_ms
        FROM targets
        WHERE enabled
        ORDER BY lower(name)
        "#
    )
    .fetch_all(db)
    .await
}
