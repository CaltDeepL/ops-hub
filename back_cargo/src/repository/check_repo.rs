//! `checks` への書き込み。

use sqlx::PgPool;
use uuid::Uuid;

use crate::provider::probe::ProbeOutcome;

/// probe の結果を1行記録する。
///
/// ## `started_at` を SQL 側で逆算している
///
/// Rust 側で `timestamptz` を組み立てるには `chrono` を直接の依存に足す必要が
/// ある（現状は sqlx のフィーチャー経由のみ）。INSERT は probe の直後なので、
/// `now() - duration` で誤差はミリ秒単位に収まる。タスク7で `idle_secs` を
/// SQL 側で秒数に落としたのと同じ方針。
///
/// ## ENUM を `$4::text::check_result` で渡している
///
/// `$4::check_result` と直接書くと PostgreSQL が `$4` 自体を `check_result` 型と
/// 推論し、sqlx が ENUM 対応の Rust 型を要求してくる。一度 `text` に落とせば
/// 素の `&str` で渡せる。
pub async fn insert(
    db: &PgPool,
    run_id: Uuid,
    target_id: Uuid,
    outcome: &ProbeOutcome,
) -> sqlx::Result<()> {
    sqlx::query!(
        r#"
        INSERT INTO checks
            (target_id, run_id, started_at, duration_ms, result, status_code, degraded, error_detail)
        VALUES
            ($1, $2, now() - $3::integer * interval '1 millisecond', $3,
             $4::text::check_result, $5, $6, $7)
        "#,
        target_id,
        run_id,
        outcome.duration_ms,
        outcome.result.as_str(),
        outcome.status_code,
        outcome.degraded,
        outcome.error_detail.as_deref()
    )
    .execute(db)
    .await?;

    Ok(())
}
