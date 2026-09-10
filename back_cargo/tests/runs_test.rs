//! `POST /v1/runs` の統合テスト（タスク6の完了条件）。
//!
//! `#[sqlx::test]` は1テストにつき1データベースを作り、`./migrations` を適用する。
//! advisory lock のスコープはデータベース単位なので、テスト同士は並列に走っても
//! 衝突しない（タスク5で実測済み）。
//!
//! プールは `max_connections(3)` で作り直している。1リクエストの最中に
//! 「ロック保持」「`runs` への INSERT」「テスト側の検証クエリ」で最大3本要る。
//! 既定値のままだと、失敗したときに「競合が再現しなかった」のか
//! 「プールが枯れて acquire がタイムアウトした」のかが区別できない。
//!
//! **背景タスクの完了を待つ検証はポーリングで行う。** ハンドラは 202 を返した
//! 時点では締めていない。`sleep` で決め打ちすると CI で不安定になる。

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt as _;
use ops_hub::config::Config;
use ops_hub::run_lock::RunLock;
use ops_hub::state::AppState;
use serde_json::Value;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{PgPool, Postgres, pool::PoolOptions};
use tower::ServiceExt as _;
use uuid::Uuid;

const KEY: i64 = 8_421_337;

// =============================================================================
// ヘルパ
// =============================================================================

async fn test_pool(
    _opts: PoolOptions<Postgres>,
    connect: PgConnectOptions,
) -> sqlx::Result<PgPool> {
    PgPoolOptions::new()
        .max_connections(3)
        .min_connections(0)
        .acquire_timeout(Duration::from_secs(5))
        .connect_with(connect)
        .await
}

/// テスト用の `Config`。`from_env` は使わない（環境変数に依存させない）。
///
/// `database_url` はここでは使われない。プールは既に張られており、
/// `run_lock_key` だけがハンドラの経路に効く。
fn test_state(db: PgPool) -> AppState {
    AppState {
        db,
        config: Arc::new(Config {
            database_url: "postgres://test/ops_hub".to_string(),
            port: 0,
            db_max_connections: 3,
            db_acquire_timeout: Duration::from_secs(5),
            run_lock_key: KEY,
        }),
    }
}

async fn post_runs(state: AppState) -> (StatusCode, Value) {
    let response = ops_hub::app(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/runs")
                .body(Body::empty())
                .expect("リクエストを組み立てられる"),
        )
        .await
        .expect("ハンドラが応答する");

    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("ボディを読み出せる")
        .to_bytes();
    let json = serde_json::from_slice(&bytes).expect("JSON として解釈できる");

    (status, json)
}

fn run_id_of(json: &Value) -> Uuid {
    let raw = json["run_id"]
        .as_str()
        .unwrap_or_else(|| panic!("run_id が文字列ではありません: {json}"));
    Uuid::parse_str(raw).expect("run_id が UUID として解釈できる")
}

/// `runs` の1行を `(status, finished_at IS NOT NULL, targets_checked)` で取る。
///
/// マクロ版（`query!`）を使わないのは、テストの実行に `.sqlx` や `DATABASE_URL`
/// の状態を巻き込まないため。
async fn fetch_run(db: &PgPool, run_id: Uuid) -> Option<(String, bool, i32)> {
    sqlx::query_as::<_, (String, bool, i32)>(
        "SELECT status::text, finished_at IS NOT NULL, targets_checked FROM runs WHERE id = $1",
    )
    .bind(run_id)
    .fetch_optional(db)
    .await
    .expect("runs を読める")
}

/// 背景タスクが締めるまで待つ。締まらなければ `None`。
async fn wait_until_finished(db: &PgPool, run_id: Uuid, limit: Duration) -> Option<(String, i32)> {
    let deadline = std::time::Instant::now() + limit;
    loop {
        if let Some((status, finished, checked)) = fetch_run(db, run_id).await
            && finished
        {
            return Some((status, checked));
        }
        if std::time::Instant::now() >= deadline {
            return None;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// 実行中の run を1件、手で作る。競合時に `run_id` が返ることの検証に使う。
async fn insert_running(db: &PgPool) -> Uuid {
    sqlx::query_scalar::<_, Uuid>("INSERT INTO runs DEFAULT VALUES RETURNING id")
        .fetch_one(db)
        .await
        .expect("runs に行を作れる")
}

// =============================================================================
// ① 起動できるとき
// =============================================================================

#[sqlx::test(migrations = "./migrations")]
async fn 起動すると202とrun_idを返しrunsに記録される(
    opts: PoolOptions<Postgres>,
    connect: PgConnectOptions,
) -> sqlx::Result<()> {
    let pool = test_pool(opts, connect).await?;

    let (status, json) = post_runs(test_state(pool.clone())).await;

    assert_eq!(status, StatusCode::ACCEPTED, "起動時は 202: {json}");
    assert_eq!(json["status"], "started");

    let run_id = run_id_of(&json);
    assert!(
        fetch_run(&pool, run_id).await.is_some(),
        "返された run_id の行が runs に無い"
    );

    Ok(())
}

#[sqlx::test(migrations = "./migrations")]
async fn 背景処理が終わるとcompletedで締められる(
    opts: PoolOptions<Postgres>,
    connect: PgConnectOptions,
) -> sqlx::Result<()> {
    let pool = test_pool(opts, connect).await?;

    let (_, json) = post_runs(test_state(pool.clone())).await;
    let run_id = run_id_of(&json);

    let (status, targets_checked) = wait_until_finished(&pool, run_id, Duration::from_secs(5))
        .await
        .expect("5秒以内に finished_at が入るはず（背景タスクが締めていない）");

    assert_eq!(status, "completed");
    // タスク6では巡回しないので 0。タスク8でここが変わる
    assert_eq!(targets_checked, 0);

    Ok(())
}

#[sqlx::test(migrations = "./migrations")]
async fn 締めたあとはロックが解放され連続で起動できる(
    opts: PoolOptions<Postgres>,
    connect: PgConnectOptions,
) -> sqlx::Result<()> {
    let pool = test_pool(opts, connect).await?;

    let (_, first) = post_runs(test_state(pool.clone())).await;
    let first_id = run_id_of(&first);
    wait_until_finished(&pool, first_id, Duration::from_secs(5))
        .await
        .expect("1回目が締まるはず");

    // 解放はサーバ側の検知に一瞬かかるので、取れるまで少し待つ
    let mut second = post_runs(test_state(pool.clone())).await;
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while second.1["status"] == "already_running" && std::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(50)).await;
        second = post_runs(test_state(pool.clone())).await;
    }

    assert_eq!(
        second.0,
        StatusCode::ACCEPTED,
        "解放後に再起動できていない: {}",
        second.1
    );
    assert_ne!(run_id_of(&second.1), first_id, "別の run になるはず");

    Ok(())
}

// =============================================================================
// ② 競合しているとき
// =============================================================================

#[sqlx::test(migrations = "./migrations")]
async fn ロック保持中は200とalready_runningを返す(
    opts: PoolOptions<Postgres>,
    connect: PgConnectOptions,
) -> sqlx::Result<()> {
    let pool = test_pool(opts, connect).await?;

    // 別プロセスが実行中の状況を再現する
    let running_id = insert_running(&pool).await;
    let lock = RunLock::try_acquire(&pool, KEY)
        .await?
        .expect("テスト側がロックを取れるはず");

    let (status, json) = post_runs(test_state(pool.clone())).await;

    assert_eq!(status, StatusCode::OK, "競合は 409 ではなく 200: {json}");
    assert_eq!(json["status"], "already_running");
    assert_eq!(
        run_id_of(&json),
        running_id,
        "実行中の run_id を返していない"
    );

    // 競合したのに新しい行を作っていないこと
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM runs")
        .fetch_one(&pool)
        .await?;
    assert_eq!(count, 1, "競合時に runs へ行を足している");

    lock.release().await?;
    Ok(())
}

#[sqlx::test(migrations = "./migrations")]
async fn 実行中のrunを引けなければrun_idはnull(
    opts: PoolOptions<Postgres>,
    connect: PgConnectOptions,
) -> sqlx::Result<()> {
    let pool = test_pool(opts, connect).await?;

    // ロックだけ保持され、runs には running が無い状態（起動直後のレース）
    let lock = RunLock::try_acquire(&pool, KEY)
        .await?
        .expect("テスト側がロックを取れるはず");

    let (status, json) = post_runs(test_state(pool.clone())).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["status"], "already_running");
    assert!(
        json["run_id"].is_null(),
        "run_id は null になるはず: {json}"
    );

    lock.release().await?;
    Ok(())
}
