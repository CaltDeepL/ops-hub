use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context as _;
use clap::{Parser, Subcommand};
use ops_hub::{app, config::Config, recovery, state::AppState};
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
    /// 取り残された advisory lock の脱出ハッチ（タスク7）。
    ///
    /// 既定では**見るだけ**。`--force` を付けたときだけ保持セッションを切る。
    /// 通常運用では使わない。`POST /v1/runs` が延々と `already_running` を
    /// 返し続けるときの最終手段。
    Unlock {
        /// 保持セッションを実際に切断する。付けなければ状態の表示のみ。
        #[arg(long)]
        force: bool,
        /// `--force` の巻き添えを避けるための下限。この秒数より長く
        /// `idle` が続いているセッションでなければ切らない。
        #[arg(long, default_value_t = 600.0)]
        min_idle_secs: f64,
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
        Command::Unlock {
            force,
            min_idle_secs,
        } => {
            init_tracing();
            match unlock(force, min_idle_secs).await {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    tracing::error!(error = format!("{e:#}"), "unlock に失敗しました");
                    ExitCode::FAILURE
                }
            }
        }
    }
}

/// 取り残された advisory lock を調べ、`--force` のときだけ解放する。
///
/// サーバを起動せず、その場で1本だけ繋いで見る。`serve()` と違って
/// `connect_lazy` にしないのは、DBに繋がらないなら「調べられなかった」を
/// 即座に返したいため。黙って空振りすると、握られていないのか繋がっていないのか
/// 区別できない。
async fn unlock(force: bool, min_idle_secs: f64) -> anyhow::Result<()> {
    let config = Config::from_env()?;

    let pool = PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(config.db_acquire_timeout)
        .connect(&config.database_url)
        .await
        .context("データベースに接続できません")?;

    let key = config.run_lock_key;
    let Some(holder) = recovery::lock_holder(&pool, key).await? else {
        tracing::info!(key, "advisory lock は誰も保持していません。回収は不要です");
        return Ok(());
    };

    tracing::info!(
        key,
        pid = holder.pid,
        state = ?holder.state,
        idle_secs = ?holder.idle_secs,
        client_addr = ?holder.client_addr,
        "advisory lock の保持セッション"
    );

    if !force {
        tracing::info!(
            "切断するには --force を付けて再実行してください。\
             state=active の場合は正常に巡回中の可能性があります"
        );
        return Ok(());
    }

    // 生きている実行を撃たないための最後の関門。idle_secs が取れない
    // （state_change が NULL）場合も、判断材料が無いので切らない。
    let idle_secs = holder.idle_secs.unwrap_or(0.0);
    if idle_secs < min_idle_secs {
        anyhow::bail!(
            "保持セッションの idle は {idle_secs:.0} 秒で、下限 {min_idle_secs:.0} 秒に達していません。\
             巡回中の可能性があります。本当に切るなら --min-idle-secs を下げてください"
        );
    }

    let terminated = recovery::terminate_holder(&pool, holder.pid).await?;
    anyhow::ensure!(terminated, "PID {} の切断に失敗しました", holder.pid);

    // ロックが落ちても runs の行は running のまま残る。次の
    // POST /v1/runs でスイーパーが拾うが、ここでも掃いておくと状態が揃う。
    let swept = recovery::sweep_stale_runs(&pool, config.run_stale_after_secs).await?;
    tracing::info!(
        swept = swept.len(),
        "advisory lock を解放しました（取り残された run も締めました）"
    );

    Ok(())
}

async fn serve() -> anyhow::Result<()> {
    let config = Arc::new(Config::from_env()?);

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

    let state = AppState { db: pool, config };

    axum::serve(listener, app(state))
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
