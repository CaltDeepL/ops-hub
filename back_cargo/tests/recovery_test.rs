//! 取り残し回収の統合テスト（タスク7の完了条件）。
//!
//! `#[sqlx::test]` は1テストにつき1データベースを作る。advisory lock も
//! `pg_locks` の可視範囲もデータベース単位ではないことに注意：`pg_locks` は
//! **クラスタ全体**を映すので、並列に走る他のテストが同じキーを握っていると
//! 見えてしまう。そのため、ロックを覗くテストはテストごとに別のキーを使う。
//!
//! 時間依存のテストは `started_at` を直接過去に書き換えて作る。`tokio::time`
//! を進めても PostgreSQL の `now()` は動かないため。

use std::time::Duration;

use ops_hub::recovery::{self, SWEPT_ERROR};
use ops_hub::run_lock::RunLock;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{PgPool, Postgres, pool::PoolOptions};
use uuid::Uuid;

/// 閾値。テストでは短くしたいが、SQL 側は秒で受けるのでそのまま渡す。
const STALE_AFTER: f64 = 600.0;

async fn two_conn_pool(
    _opts: PoolOptions<Postgres>,
    connect: PgConnectOptions,
) -> sqlx::Result<PgPool> {
    PgPoolOptions::new()
        .max_connections(2)
        .min_connections(0)
        .acquire_timeout(Duration::from_secs(5))
        .connect_with(connect)
        .await
}

/// `running` の run を1件作る。`started_at` を `age_secs` 秒だけ過去にずらす。
async fn insert_running_aged(db: &PgPool, age_secs: i64) -> sqlx::Result<Uuid> {
    sqlx::query_scalar(
        r#"
        INSERT INTO runs (started_at)
        VALUES (now() - make_interval(secs => $1::double precision))
        RETURNING id
        "#,
    )
    .bind(age_secs as f64)
    .fetch_one(db)
    .await
}

async fn status_of(db: &PgPool, run_id: Uuid) -> sqlx::Result<(String, Option<String>, bool)> {
    let row: (String, Option<String>, bool) = sqlx::query_as(
        r#"
        SELECT status::text, error, (finished_at IS NOT NULL)
        FROM runs WHERE id = $1
        "#,
    )
    .bind(run_id)
    .fetch_one(db)
    .await?;
    Ok(row)
}

// =============================================================================
// スイーパー
// =============================================================================

#[sqlx::test]
async fn 閾値を超えたrunをfailedに倒す(db: PgPool) -> sqlx::Result<()> {
    let stale = insert_running_aged(&db, 601).await?;

    let swept = recovery::sweep_stale_runs(&db, STALE_AFTER).await?;
    assert_eq!(swept, vec![stale], "閾値を超えた run が掃かれていない");

    let (status, error, finished) = status_of(&db, stale).await?;
    assert_eq!(status, "failed");
    assert_eq!(error.as_deref(), Some(SWEPT_ERROR));
    assert!(finished, "finished_at が埋まっていない");

    Ok(())
}

#[sqlx::test]
async fn 閾値内のrunには触らない(db: PgPool) -> sqlx::Result<()> {
    let fresh = insert_running_aged(&db, 59).await?;

    let swept = recovery::sweep_stale_runs(&db, STALE_AFTER).await?;
    assert!(swept.is_empty(), "走行中の run を巻き込んでいる: {swept:?}");

    let (status, error, finished) = status_of(&db, fresh).await?;
    assert_eq!(status, "running");
    assert_eq!(error, None);
    assert!(!finished);

    Ok(())
}

#[sqlx::test]
async fn 既に締まったrunは対象外(db: PgPool) -> sqlx::Result<()> {
    let old = insert_running_aged(&db, 9_999).await?;
    sqlx::query("UPDATE runs SET status = 'completed', finished_at = now() WHERE id = $1")
        .bind(old)
        .execute(&db)
        .await?;

    let swept = recovery::sweep_stale_runs(&db, STALE_AFTER).await?;
    assert!(
        swept.is_empty(),
        "completed の run を failed に倒してしまっている"
    );

    let (status, _, _) = status_of(&db, old).await?;
    assert_eq!(status, "completed", "完了済みの記録が書き換えられた");

    Ok(())
}

#[sqlx::test]
async fn 対象が無ければ何もしない(db: PgPool) -> sqlx::Result<()> {
    let swept = recovery::sweep_stale_runs(&db, STALE_AFTER).await?;
    assert!(swept.is_empty());
    Ok(())
}

#[sqlx::test]
async fn 複数の取り残しをまとめて倒す(db: PgPool) -> sqlx::Result<()> {
    // 本来 running は同時に1件だが、プロセスが複数回落ちれば溜まりうる
    let a = insert_running_aged(&db, 700).await?;
    let b = insert_running_aged(&db, 3_600).await?;
    let fresh = insert_running_aged(&db, 10).await?;

    let mut swept = recovery::sweep_stale_runs(&db, STALE_AFTER).await?;
    swept.sort();
    let mut expected = vec![a, b];
    expected.sort();
    assert_eq!(swept, expected);

    let (status, _, _) = status_of(&db, fresh).await?;
    assert_eq!(status, "running");

    Ok(())
}

/// スイーパーが倒した後に本来の締めが来ても、`failed` を `completed` で
/// 上書きしない（`finish_completed` の `WHERE status = 'running'`）。
#[sqlx::test]
async fn スイーパーが倒した後の締めは空振りする(db: PgPool) -> sqlx::Result<()> {
    let stale = insert_running_aged(&db, 700).await?;
    recovery::sweep_stale_runs(&db, STALE_AFTER).await?;

    let finished = ops_hub::repository::run_repo::finish_completed(&db, stale, 3).await?;
    assert!(!finished, "failed を completed で上書きしてしまっている");

    let (status, error, _) = status_of(&db, stale).await?;
    assert_eq!(status, "failed");
    assert_eq!(error.as_deref(), Some(SWEPT_ERROR));

    Ok(())
}

// =============================================================================
// 脱出ハッチ（advisory lock の保持セッション）
// =============================================================================

#[sqlx::test]
async fn 誰も握っていなければ_noneを返す(
    opts: PoolOptions<Postgres>,
    connect: PgConnectOptions,
) -> sqlx::Result<()> {
    let pool = two_conn_pool(opts, connect).await?;
    // 他テストと衝突しないよう固有のキーを使う
    let key = 7_000_001;

    assert!(recovery::lock_holder(&pool, key).await?.is_none());
    Ok(())
}

#[sqlx::test]
async fn 保持中のセッションを引ける(
    opts: PoolOptions<Postgres>,
    connect: PgConnectOptions,
) -> sqlx::Result<()> {
    let pool = two_conn_pool(opts, connect).await?;
    let key = 7_000_002;

    let lock = RunLock::try_acquire(&pool, key)
        .await?
        .expect("取得できるはず");

    let holder = recovery::lock_holder(&pool, key)
        .await?
        .expect("保持中なのに引けていない。lock_key_parts のビット演算を疑う");
    assert!(holder.pid > 0, "PID が取れていない");

    lock.release().await?;

    // 解放後は見えなくなる
    assert!(
        recovery::lock_holder(&pool, key).await?.is_none(),
        "release 後も保持セッションが見えている"
    );

    Ok(())
}

/// `lock_key_parts` が上位32bitを正しく扱えているか、実DB越しに確かめる。
/// ユニットテストはビット演算しか見ていないので、`::oid` キャストまで通す。
#[sqlx::test]
async fn 上位ビットのあるキーでも引ける(
    opts: PoolOptions<Postgres>,
    connect: PgConnectOptions,
) -> sqlx::Result<()> {
    let pool = two_conn_pool(opts, connect).await?;
    let key = (3_i64 << 32) | 7_000_003;

    let lock = RunLock::try_acquire(&pool, key)
        .await?
        .expect("取得できるはず");
    assert!(
        recovery::lock_holder(&pool, key).await?.is_some(),
        "classid（上位32bit）の突き合わせが外れている"
    );
    lock.release().await?;

    Ok(())
}

/// 切断でロックが落ちること。`terminate_holder` の実地確認。
#[sqlx::test]
async fn 切断するとロックが解放される(
    opts: PoolOptions<Postgres>,
    connect: PgConnectOptions,
) -> sqlx::Result<()> {
    let pool = two_conn_pool(opts, connect).await?;
    let key = 7_000_004;

    let lock = RunLock::try_acquire(&pool, key)
        .await?
        .expect("取得できるはず");
    let holder = recovery::lock_holder(&pool, key)
        .await?
        .expect("引けるはず");

    assert!(recovery::terminate_holder(&pool, holder.pid).await?);

    // 切断の検知に一瞬かかる
    let mut reacquired = None;
    for _ in 0..60 {
        if let Some(l) = RunLock::try_acquire(&pool, key).await? {
            reacquired = Some(l);
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let reacquired = reacquired.expect("切断後もロックが解放されていない");
    reacquired.release().await?;

    // 切られた側の release は失敗しうる。落ちないことだけ確かめる
    let _ = lock.release().await;

    Ok(())
}

#[sqlx::test]
async fn 存在しない_pidの切断はfalse(db: PgPool) -> sqlx::Result<()> {
    // 使われていないであろう高い PID。存在しても切れないだけで害は無い
    let terminated = recovery::terminate_holder(&db, 2_147_483_647).await?;
    assert!(!terminated);
    Ok(())
}
