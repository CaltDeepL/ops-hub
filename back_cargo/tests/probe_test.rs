//! タスク8：対象の巡回（targets 取得 → probe → checks 記録）。
//!
//! ## テストを2層に分けている理由
//!
//! `targets` には `targets_url_https` CHECK があり `https://` で始まる URL しか
//! 入らない。ローカルに立てるテストサーバは平文 HTTP なので、**DB を経由する
//! 経路ではテストサーバを叩けない。**
//!
//! 1. `probe` 単体：`Target` を Rust 側で組み立てて CHECK を迂回し、ローカルの
//!    axum サーバへ実際に投げる
//! 2. 通し（`POST /v1/runs` → `checks`）：`https://` のまま**誰も listen して
//!    いないポート**を指し、`connection_error` が記録されることで配線を確かめる
//!
//! テストサーバのポートは `127.0.0.1:0` で OS に選ばせる。固定ポートは
//! 並列実行で衝突する。

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::response::Redirect;
use axum::routing::get;
use ops_hub::config::Config;
use ops_hub::provider::probe::{self, CheckResult};
use ops_hub::repository::target_repo::{self, Target};
use ops_hub::state::AppState;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{PgPool, Postgres, pool::PoolOptions};
use tokio::net::TcpListener;
use tower::ServiceExt as _;
use uuid::Uuid;

// =============================================================================
// ヘルパ
// =============================================================================

/// probe の相手になるローカルサーバを立て、ベース URL を返す。
async fn spawn_server() -> String {
    let app = Router::new()
        .route("/ok", get(|| async { "ok" }))
        .route(
            "/fail",
            get(|| async { (StatusCode::SERVICE_UNAVAILABLE, "down") }),
        )
        .route(
            "/lazy",
            get(|| async {
                tokio::time::sleep(Duration::from_millis(300)).await;
                "late"
            }),
        )
        .route(
            "/slow",
            get(|| async {
                tokio::time::sleep(Duration::from_secs(10)).await;
                "too late"
            }),
        )
        .route("/redirect", get(|| async { Redirect::permanent("/ok") }));

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("テストサーバを待ち受けられる");
    let addr = listener.local_addr().expect("アドレスを取れる");
    tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("テストサーバが動く");
    });

    format!("http://{addr}")
}

/// 誰も listen していないポートを1つ返す。
///
/// 取って即座に閉じる。閉じてから接続するまでの間に別プロセスが同じポートを
/// 掴む理論上の可能性はあるが、OS はエフェメラルポートを順に払い出すので
/// 実用上は起きない。
async fn closed_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("ポートを取れる");
    listener.local_addr().expect("アドレスを取れる").port()
}

fn target(url: String) -> Target {
    Target {
        id: Uuid::new_v4(),
        name: "test".to_string(),
        url,
        method: "GET".to_string(),
        expected_status: 200,
        timeout_ms: 5_000,
        degraded_threshold_ms: 4_000,
    }
}

fn client() -> reqwest::Client {
    probe::build_client().expect("HTTP クライアントを作れる")
}

// =============================================================================
// 1. probe 単体
// =============================================================================

#[tokio::test]
async fn 期待どおりのステータスならsuccess() {
    let base = spawn_server().await;
    let outcome = probe::probe(&client(), &target(format!("{base}/ok"))).await;

    assert_eq!(outcome.result, CheckResult::Success);
    assert_eq!(outcome.status_code, Some(200));
    assert!(!outcome.degraded);
    assert_eq!(outcome.error_detail, None);
}

#[tokio::test]
async fn 期待と違うステータスならhttp_error() {
    let base = spawn_server().await;
    let outcome = probe::probe(&client(), &target(format!("{base}/fail"))).await;

    assert_eq!(outcome.result, CheckResult::HttpError);
    assert_eq!(outcome.status_code, Some(503));
    assert!(outcome.error_detail.is_some());
}

#[tokio::test]
async fn expected_statusに一致すれば5xxでもsuccess() {
    let base = spawn_server().await;
    let mut t = target(format!("{base}/fail"));
    t.expected_status = 503;

    let outcome = probe::probe(&client(), &t).await;

    assert_eq!(outcome.result, CheckResult::Success);
}

#[tokio::test]
async fn 閾値超えのsuccessはdegraded() {
    let base = spawn_server().await;
    let mut t = target(format!("{base}/lazy"));
    t.degraded_threshold_ms = 50;

    let outcome = probe::probe(&client(), &t).await;

    assert_eq!(outcome.result, CheckResult::Success);
    assert!(
        outcome.degraded,
        "300ms 待ちが閾値50msを超えている: {outcome:?}"
    );
    assert!(outcome.duration_ms >= 300);
}

#[tokio::test]
async fn 時間切れならtimeout() {
    let base = spawn_server().await;
    let mut t = target(format!("{base}/slow"));
    t.timeout_ms = 1_000;
    t.degraded_threshold_ms = 500;

    let outcome = probe::probe(&client(), &t).await;

    assert_eq!(outcome.result, CheckResult::Timeout);
    assert_eq!(outcome.status_code, None);
    assert!(outcome.duration_ms >= 1_000);
    assert!(!outcome.degraded, "失敗は degraded にしない");
}

#[tokio::test]
async fn 繋がらなければconnection_error() {
    let port = closed_port().await;
    let outcome = probe::probe(&client(), &target(format!("http://127.0.0.1:{port}/"))).await;

    assert_eq!(outcome.result, CheckResult::ConnectionError);
    assert_eq!(outcome.status_code, None);
    assert!(outcome.error_detail.is_some());
}

#[tokio::test]
async fn headでも判定できる() {
    let base = spawn_server().await;
    let mut t = target(format!("{base}/ok"));
    t.method = "HEAD".to_string();

    let outcome = probe::probe(&client(), &t).await;

    assert_eq!(outcome.result, CheckResult::Success);
    assert_eq!(outcome.status_code, Some(200));
}

#[tokio::test]
async fn リダイレクトは追わない() {
    let base = spawn_server().await;
    let outcome = probe::probe(&client(), &target(format!("{base}/redirect"))).await;

    // 追っていれば /ok の 200 で success になってしまう
    assert_eq!(outcome.result, CheckResult::HttpError);
    assert_eq!(outcome.status_code, Some(308));
}

#[tokio::test]
async fn エラー詳細にクエリ文字列を残さない() {
    let port = closed_port().await;
    let url = format!("http://user:pass@127.0.0.1:{port}/health?token=secret");

    let outcome = probe::probe(&client(), &target(url)).await;
    let detail = outcome.error_detail.expect("失敗時は詳細がある");

    assert!(!detail.contains("secret"), "{detail}");
    assert!(!detail.contains("pass"), "{detail}");
}

// =============================================================================
// 2. 通し（DB を経由する経路）
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

fn test_state(db: PgPool) -> AppState {
    AppState {
        db,
        config: Arc::new(Config {
            database_url: "postgres://test/ops_hub".to_string(),
            port: 0,
            db_max_connections: 3,
            db_acquire_timeout: Duration::from_secs(5),
            run_lock_key: 8_421_337,
            run_stale_after_secs: 600.0,
            probe_concurrency: 2,
        }),
        http: client(),
    }
}

async fn insert_target(db: &PgPool, name: &str, url: &str, enabled: bool) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO targets (name, url, severity, enabled, timeout_ms, degraded_threshold_ms)
         VALUES ($1, $2, 'sev2', $3, 5000, 4000)
         RETURNING id",
    )
    .bind(name)
    .bind(url)
    .bind(enabled)
    .fetch_one(db)
    .await
    .expect("targets に行を作れる")
}

/// `POST /v1/runs` を叩き、背景処理が締まるまで待って `run_id` を返す。
async fn run_once(db: &PgPool) -> Uuid {
    let response = ops_hub::app(test_state(db.clone()))
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/runs")
                .body(Body::empty())
                .expect("リクエストを組み立てられる"),
        )
        .await
        .expect("ハンドラが応答する");
    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let bytes = http_body_util::BodyExt::collect(response.into_body())
        .await
        .expect("ボディを読める")
        .to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).expect("JSON");
    let run_id = Uuid::parse_str(json["run_id"].as_str().expect("run_id")).expect("UUID");

    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        let finished: bool =
            sqlx::query_scalar("SELECT finished_at IS NOT NULL FROM runs WHERE id = $1")
                .bind(run_id)
                .fetch_one(db)
                .await
                .expect("runs を読める");
        if finished {
            return run_id;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "10秒以内に締まらなかった"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn 有効な対象を巡回してchecksに記録する(
    opts: PoolOptions<Postgres>,
    connect: PgConnectOptions,
) -> sqlx::Result<()> {
    let pool = test_pool(opts, connect).await?;
    let port = closed_port().await;
    let a = insert_target(&pool, "alpha", &format!("https://127.0.0.1:{port}/a"), true).await;
    let b = insert_target(&pool, "bravo", &format!("https://127.0.0.1:{port}/b"), true).await;
    let c = insert_target(
        &pool,
        "charlie",
        &format!("https://127.0.0.1:{port}/c"),
        true,
    )
    .await;

    let run_id = run_once(&pool).await;

    let (status, checked): (String, i32) =
        sqlx::query_as("SELECT status::text, targets_checked FROM runs WHERE id = $1")
            .bind(run_id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(status, "completed", "対象が落ちていても run は completed");
    assert_eq!(checked, 3);

    let mut rows: Vec<(Uuid, String, Option<i32>, bool)> = sqlx::query_as(
        "SELECT target_id, result::text, status_code, error_detail IS NOT NULL
         FROM checks WHERE run_id = $1",
    )
    .bind(run_id)
    .fetch_all(&pool)
    .await?;
    rows.sort_by_key(|row| row.0);
    let mut expected = vec![a, b, c];
    expected.sort();

    assert_eq!(rows.iter().map(|r| r.0).collect::<Vec<_>>(), expected);
    for (_, result, status_code, has_detail) in rows {
        assert_eq!(result, "connection_error");
        assert_eq!(status_code, None);
        assert!(has_detail);
    }

    Ok(())
}

#[sqlx::test(migrations = "./migrations")]
async fn 無効な対象は巡回しない(
    opts: PoolOptions<Postgres>,
    connect: PgConnectOptions,
) -> sqlx::Result<()> {
    let pool = test_pool(opts, connect).await?;
    let port = closed_port().await;
    insert_target(
        &pool,
        "enabled",
        &format!("https://127.0.0.1:{port}/"),
        true,
    )
    .await;
    let disabled = insert_target(
        &pool,
        "disabled",
        &format!("https://127.0.0.1:{port}/"),
        false,
    )
    .await;

    let run_id = run_once(&pool).await;

    let checked: i32 = sqlx::query_scalar("SELECT targets_checked FROM runs WHERE id = $1")
        .bind(run_id)
        .fetch_one(&pool)
        .await?;
    assert_eq!(checked, 1);

    let disabled_checks: i64 =
        sqlx::query_scalar("SELECT count(*) FROM checks WHERE target_id = $1")
            .bind(disabled)
            .fetch_one(&pool)
            .await?;
    assert_eq!(disabled_checks, 0);

    Ok(())
}

#[sqlx::test(migrations = "./migrations")]
async fn 対象一覧は名前順(
    opts: PoolOptions<Postgres>,
    connect: PgConnectOptions,
) -> sqlx::Result<()> {
    let pool = test_pool(opts, connect).await?;
    for name in ["charlie", "Alpha", "bravo"] {
        insert_target(&pool, name, "https://example.invalid/", true).await;
    }

    let names: Vec<String> = target_repo::list_enabled(&pool)
        .await?
        .into_iter()
        .map(|t| t.name)
        .collect();

    assert_eq!(names, ["Alpha", "bravo", "charlie"]);
    Ok(())
}

#[sqlx::test(migrations = "./migrations")]
async fn started_atはprobe開始時刻に逆算される(
    opts: PoolOptions<Postgres>,
    connect: PgConnectOptions,
) -> sqlx::Result<()> {
    let pool = test_pool(opts, connect).await?;
    let port = closed_port().await;
    insert_target(&pool, "alpha", &format!("https://127.0.0.1:{port}/"), true).await;

    let run_id = run_once(&pool).await;

    // started_at は run の開始以降、かつ「started_at + duration」が現在以前
    let consistent: bool = sqlx::query_scalar(
        "SELECT c.started_at >= r.started_at - interval '1 second'
            AND c.started_at + c.duration_ms * interval '1 millisecond' <= now()
         FROM checks c JOIN runs r ON r.id = c.run_id
         WHERE c.run_id = $1",
    )
    .bind(run_id)
    .fetch_one(&pool)
    .await?;
    assert!(consistent);

    Ok(())
}

#[sqlx::test(migrations = "./migrations")]
async fn 長いエラー詳細も512文字に詰めて記録できる(
    opts: PoolOptions<Postgres>,
    connect: PgConnectOptions,
) -> sqlx::Result<()> {
    let pool = test_pool(opts, connect).await?;
    let port = closed_port().await;
    // エラー表示に URL が載るので、パスを長くすれば詳細も512文字を超える
    let long_path = "x".repeat(1_000);
    insert_target(
        &pool,
        "long",
        &format!("https://127.0.0.1:{port}/{long_path}"),
        true,
    )
    .await;

    let run_id = run_once(&pool).await;

    let checked: i32 = sqlx::query_scalar("SELECT targets_checked FROM runs WHERE id = $1")
        .bind(run_id)
        .fetch_one(&pool)
        .await?;
    assert_eq!(checked, 1, "checks_error_detail_len で INSERT が落ちている");

    let len: i32 = sqlx::query_scalar("SELECT length(error_detail) FROM checks WHERE run_id = $1")
        .bind(run_id)
        .fetch_one(&pool)
        .await?;
    assert!(len <= 512, "{len}");

    Ok(())
}
