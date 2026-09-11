//! `RunLock` の統合テスト（タスク5の完了条件）。
//!
//! `#[sqlx::test]` は1テストにつき1データベースを作る。advisory lock のスコープは
//! データベース単位なので、テスト同士は並列に走っても衝突しない（基本設計10章の宿題8）。
//!
//! プールは明示的に `max_connections(2)` で作り直している。既定値のままだと
//! 「競合が再現しなかった」のか「プールが1本しか張れず acquire がタイムアウトした」
//! のかが区別できず、失敗時の切り分けに時間を取られるため。

use std::time::Duration;

use ops_hub::run_lock::RunLock;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{PgPool, Postgres, pool::PoolOptions};

const KEY: i64 = 8_421_337;

/// 同一DBに対して2本まで張れるプールを作る。
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

#[sqlx::test]
async fn 保持中は別コネクションから取得できない(
    opts: PoolOptions<Postgres>,
    connect: PgConnectOptions,
) -> sqlx::Result<()> {
    let pool = two_conn_pool(opts, connect).await?;

    let first = RunLock::try_acquire(&pool, KEY)
        .await?
        .expect("1本目は取得できるはず");

    let second = RunLock::try_acquire(&pool, KEY).await?;
    assert!(
        second.is_none(),
        "保持中にもかかわらず2本目が取得できた。プーラ越しの接続になっていないか確認する"
    );

    first.release().await?;
    Ok(())
}

#[sqlx::test]
async fn release後に再取得できる(
    opts: PoolOptions<Postgres>,
    connect: PgConnectOptions,
) -> sqlx::Result<()> {
    let pool = two_conn_pool(opts, connect).await?;

    let first = RunLock::try_acquire(&pool, KEY)
        .await?
        .expect("1本目は取得できるはず");
    first.release().await?;

    let second = RunLock::try_acquire(&pool, KEY)
        .await?
        .expect("release 後は再取得できるはず");
    second.release().await?;

    Ok(())
}

#[sqlx::test]
async fn キーが違えば競合しない(
    opts: PoolOptions<Postgres>,
    connect: PgConnectOptions,
) -> sqlx::Result<()> {
    let pool = two_conn_pool(opts, connect).await?;

    let a = RunLock::try_acquire(&pool, KEY).await?.expect("A は取れる");
    let b = RunLock::try_acquire(&pool, KEY + 1)
        .await?
        .expect("別キーの B も取れる");

    assert_eq!(a.key(), KEY);
    assert_eq!(b.key(), KEY + 1);

    a.release().await?;
    b.release().await?;
    Ok(())
}

/// `release()` を呼ばずに落ちた場合の保険（`Drop` で切断）が効いているか。
///
/// 切断をサーバが検知するまでに一瞬かかるので、即座に再取得を試すと落ちる。
/// 短い間隔でポーリングして待つ。
#[sqlx::test]
async fn releaseせずにdropしても解放される(
    opts: PoolOptions<Postgres>,
    connect: PgConnectOptions,
) -> sqlx::Result<()> {
    let pool = two_conn_pool(opts, connect).await?;

    {
        let lock = RunLock::try_acquire(&pool, KEY)
            .await?
            .expect("取得できるはず");
        drop(lock); // release を呼ばない
    }

    let reacquired = acquire_within(&pool, KEY, Duration::from_secs(3)).await?;
    let reacquired =
        reacquired.expect("drop 後にロックが解放されていない（Drop の detach を確認する）");
    reacquired.release().await?;

    Ok(())
}

/// 指定時間まで再試行しながら取得する。テスト専用。
async fn acquire_within(pool: &PgPool, key: i64, limit: Duration) -> sqlx::Result<Option<RunLock>> {
    let deadline = std::time::Instant::now() + limit;
    loop {
        if let Some(lock) = RunLock::try_acquire(pool, key).await? {
            return Ok(Some(lock));
        }
        if std::time::Instant::now() >= deadline {
            return Ok(None);
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}
