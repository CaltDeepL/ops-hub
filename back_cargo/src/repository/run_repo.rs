//! `runs` テーブルへの読み書き。
//!
//! `runs` は観測用の記録であり、排他制御の手段ではない（基本設計 3.2）。
//! 排他は [`RunLock`](crate::run_lock::RunLock) が advisory lock で行う。
//!
//! ## `run_status` を Rust の enum にしていない理由
//!
//! この層では状態を **SQL のリテラルとしてしか扱わない**。`'running'` /
//! `'completed'` は書き込み時に固定値で、読み出しも「実行中の run があるか」の
//! 判定だけ。Rust 側に `#[derive(sqlx::Type)]` の写像を作ると、
//! `SELECT status as "status: RunStatus"` のキャスト注釈と `.sqlx` の再生成が
//! 必要になる割に、得られるものが無い。ステータスを値として持ち回る必要が
//! 出るのは `GET /v1/runs`（履歴の返却）からなので、その時に入れる。

use sqlx::PgPool;
use uuid::Uuid;

/// 実行中（`running`）の行を1件作り、その ID を返す。
///
/// `status` / `started_at` / `targets_checked` / `notifications_sent` は
/// すべて DEFAULT が効くので、明示的に列を並べない。列が増えたときに
/// ここを直し忘れる余地を無くしておく。
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
/// ようにしておく。advisory lock の取得と `runs` の INSERT は原子的な1操作では
/// ないため、ロックだけ保持されていて `running` 行が無い瞬間がある。そのときは
/// `None`（＝ `run_id: null`）を返す（T6-3）。
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

/// 中身の無い run を正常終了として閉じる（T6-1）。締められたら `true`。
///
/// タスク06の背景処理は no-op だが、`running` のまま残すとタスク07の
/// 「直近15分の running」判定と将来の実行履歴を汚す。**no-op が正常終了した
/// run として `finished_at` を入れる。**
///
/// `targets_checked` / `notifications_sent` は DEFAULT の 0 のままでよいので
/// 触らない。タスク08でこの関数は、実際の巡回結果を受け取る完了処理
/// （`complete(run_id, targets_checked, notifications_sent)`）と、失敗時に
/// `failed` へ倒す `fail(run_id, error)` に置き換わる。
///
/// `WHERE status = 'running'` を付けているのは、タスク07のスイーパーが先に
/// `failed` へ倒していた場合に、それを上書きして無かったことにしないため。
/// 戻り値が `false` のときは「別の主体が既に締めていた」を意味する。
pub async fn complete_empty(db: &PgPool, run_id: Uuid) -> sqlx::Result<bool> {
    let result = sqlx::query!(
        r#"
        UPDATE runs
        SET status      = 'completed',
            finished_at = now()
        WHERE id = $1
          AND status = 'running'
        "#,
        run_id
    )
    .execute(db)
    .await?;

    Ok(result.rows_affected() == 1)
}
