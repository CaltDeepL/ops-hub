//! `runs` テーブルへの読み書き。
//!
//! `runs` は観測用の記録であり、排他制御の手段ではない（基本設計 3.2）。
//! 排他は [`RunLock`](crate::run_lock::RunLock) が advisory lock で行う。
//!
//! ## `run_status` を Rust の enum にしていない理由
//!
//! この層では状態を **SQL のリテラルとしてしか扱わない**。`'running'` /
//! `'completed'` / `'failed'` は書き込み時に固定値で、読み出しも「実行中の run が
//! あるか」の判定だけ。Rust 側に `#[derive(sqlx::Type)]` の写像を作ると、
//! `SELECT status as "status: RunStatus"` のキャスト注釈と `.sqlx` の再生成が
//! 必要になる割に、得られるものが無い。ステータスを値として持ち回る必要が
//! 出るのは `GET /v1/runs`（履歴の返却）からなので、その時に入れる。

use sqlx::PgPool;
use uuid::Uuid;

/// 実行中（`running`）の行を1件作り、その ID を返す。
///
/// `status` / `started_at` はどちらも DEFAULT が効くので、明示的に列を並べない。
/// 列が増えたときにここを直し忘れる余地を無くしておく。
pub async fn insert_running(db: &PgPool) -> sqlx::Result<Uuid> {
    sqlx::query_scalar!(
        r#"
        INSERT INTO runs DEFAULT VALUES
        RETURNING id
        "#
    )
    .fetch_one(db)
    .await
}

/// 実行中の run を1件引く。ロック競合時に `run_id` を返すために使う（詳細設計 2.2）。
///
/// `running` は同時に高々1件だが、`LIMIT 1` を付けて「万一2件あっても壊れない」
/// ようにしておく。ロック取得と本関数の間には隙間があり、直前に実行が終わって
/// いれば `None` になる。これはレースとして仕様に織り込み済み（`run_id: null`）。
pub async fn latest_running(db: &PgPool) -> sqlx::Result<Option<Uuid>> {
    sqlx::query_scalar!(
        r#"
        SELECT id
        FROM runs
        WHERE status = 'running'
        ORDER BY started_at DESC
        LIMIT 1
        "#
    )
    .fetch_optional(db)
    .await
}

/// 正常終了として締める。締められたら `true`。
///
/// `WHERE status = 'running'` を付けているのは、タスク7のスイーパーが先に
/// `failed` へ倒していた場合に、それを上書きして無かったことにしないため。
/// 戻り値が `false` のときは「別の主体が既に締めていた」を意味する。
pub async fn finish_completed(
    db: &PgPool,
    run_id: Uuid,
    targets_checked: i32,
) -> sqlx::Result<bool> {
    let result = sqlx::query!(
        r#"
        UPDATE runs
        SET status          = 'completed',
            finished_at     = now(),
            targets_checked = $2
        WHERE id = $1
          AND status = 'running'
        "#,
        run_id,
        targets_checked
    )
    .execute(db)
    .await?;

    Ok(result.rows_affected() == 1)
}

/// 異常終了として締める。締められたら `true`。
///
/// `error` は呼び出し側で長さを詰めてから渡す。`runs.error` に長さ制約は
/// 無いが、Slack 通知やステータス画面にそのまま出る可能性がある。
pub async fn finish_failed(db: &PgPool, run_id: Uuid, error: &str) -> sqlx::Result<bool> {
    let result = sqlx::query!(
        r#"
        UPDATE runs
        SET status      = 'failed',
            finished_at = now(),
            error       = $2
        WHERE id = $1
          AND status = 'running'
        "#,
        run_id,
        error
    )
    .execute(db)
    .await?;

    Ok(result.rows_affected() == 1)
}
