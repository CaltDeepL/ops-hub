use std::process::ExitCode;
use std::time::Duration;

use anyhow::Context as _;
use clap::{Parser, Subcommand};
use ops_hub::{app, config::Config, state::AppState};
use sqlx::postgres::PgPoolOptions;
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(name = "ops-hub", version, about = "監視・通知ハブ")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// HTTPサーバを起動する（サブコマンド省略時の既定）
    Serve,
    /// `/health` を叩き、終了コードで結果を返す（コンテナの HEALTHCHECK 用）
    Healthcheck {
        #[arg(long, env = "PORT", default_value_t = 8080)]
        port: u16,
        #[arg(long, default_value_t = 3)]
        timeout_secs: u64,
    },
}

#[tokio::main]
async fn main() -> ExitCode {
    match Cli::parse().command.unwrap_or(Command::Serve) {
        Command::Serve => {
            init_tracing();
            match serve().await {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    tracing::error!(error = format!("{e:#}"), "起動に失敗しました");
                    ExitCode::FAILURE
                }
            }
        }
        // HEALTHCHECK から毎回呼ばれるので、ログ初期化はしない（出力を汚さない）
        Command::Healthcheck { port, timeout_secs } => {
            match healthcheck(port, timeout_secs).await {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("healthcheck failed: {e:#}");
                    ExitCode::FAILURE
                }
            }
        }
    }
}

async fn serve() -> anyhow::Result<()> {
    let config = Config::from_env()?;

    tracing::info!(
        port = config.port,
        database = config.redacted_database_url(),
        "ops-hub を起動します"
    );

    if config.is_pooled_endpoint() {
        tracing::warn!(
            "DATABASE_URL がプール済みエンドポイント(-pooler)を指しています。\
             セッションレベルの advisory lock はトランザクションプーラ越しでは機能しません"
        );
    }

    // connect_lazy: 起動時にDBへ繋ぎにいかない。
    // 実行モデル上プロセスはスピンダウンから何度も起き直すため、
    // 「DBが一時的に不在だと起動そのものが失敗する」状態を避ける。
    // DBの状態は /health が 503 で表現する。
    let pool = PgPoolOptions::new()
        .max_connections(config.db_max_connections)
        .acquire_timeout(config.db_acquire_timeout)
        .connect_lazy(&config.database_url)
        .context("コネクションプールの初期化に失敗しました")?;

    let listener = TcpListener::bind(("0.0.0.0", config.port))
        .await
        .with_context(|| format!("ポート {} を待ち受けられません", config.port))?;

    tracing::info!(addr = %listener.local_addr()?, "待ち受けを開始しました");

    axum::serve(listener, app(AppState { db: pool }))
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("HTTPサーバが異常終了しました")?;

    tracing::info!("正常に終了しました");
    Ok(())
}

async fn healthcheck(port: u16, timeout_secs: u64) -> anyhow::Result<()> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .build()?;

    let status = client
        .get(format!("http://127.0.0.1:{port}/health"))
        .send()
        .await
        .context("/health に到達できません")?
        .status();

    anyhow::ensure!(status.is_success(), "/health が {status} を返しました");
    Ok(())
}

/// SIGTERM と Ctrl-C の両方で落とす。
///
/// 本番の停止も compose の `down` も SIGTERM で来る。処理中のチェックを
/// 中途半端に切らないよう、Axum の graceful shutdown に渡す。
async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        use tokio::signal::unix::{SignalKind, signal};
        match signal(SignalKind::terminate()) {
            Ok(mut s) => {
                s.recv().await;
            }
            Err(e) => tracing::error!(error = %e, "SIGTERM ハンドラを登録できませんでした"),
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => tracing::info!(signal = "SIGINT", "終了シグナルを受信しました"),
        _ = terminate => tracing::info!(signal = "SIGTERM", "終了シグナルを受信しました"),
    }
}

/// 構造化ログ（N-12）。既定はJSON、ローカルでの可読性が欲しいときは
/// `LOG_FORMAT=pretty` を指定する。
fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,sqlx=warn,tower_http=info"));

    let pretty = std::env::var("LOG_FORMAT").is_ok_and(|v| v == "pretty");
    let builder = tracing_subscriber::fmt().with_env_filter(filter);

    if pretty {
        builder.init();
    } else {
        builder.json().flatten_event(true).init();
    }
}
