//! 設定。すべて環境変数から読む。
//!
//! CLI フラグを受け付けないのは、本番（Render）でもローカル（compose）でも
//! 設定は環境変数で渡されるため。入口を1本にしておくと「フラグと環境変数の
//! どちらが効いているのか」を運用時に考えなくて済む。

use std::str::FromStr;
use std::time::Duration;

use anyhow::{Context as _, anyhow};

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub port: u16,
    pub db_max_connections: u32,
    pub db_acquire_timeout: Duration,
    /// advisory lock のキー。DB単位のスコープなので環境ごとに変える必要はない。
    pub run_lock_key: i64,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let database_url =
            std::env::var("DATABASE_URL").context("DATABASE_URL が設定されていません")?;
        let run_lock_key = parse_env("RUN_LOCK_KEY", 8_421_337_i64)?;
        // 負の値を許すと pg_locks からキーを復元する際に符号拡張が必要になり、
        // 脱出ハッチ（タスク7）のSQLが静かに壊れる。入口で弾く。
        if run_lock_key <= 0 {
            return Err(anyhow!("環境変数 RUN_LOCK_KEY は正の整数にしてください"));
        }

        Ok(Self {
            database_url,
            port: parse_env("PORT", 8080)?,
            db_max_connections: parse_env("DB_MAX_CONNECTIONS", 5)?,
            db_acquire_timeout: Duration::from_secs(parse_env("DB_ACQUIRE_TIMEOUT_SECS", 5)?),
            run_lock_key,
        })
    }

    /// ログ出力用にパスワードを伏せた接続文字列を返す（N-8）。
    pub fn redacted_database_url(&self) -> String {
        redact_url(&self.database_url)
    }

    /// Neon のプール済みエンドポイント（ホスト名に `-pooler` を含む）かどうか。
    ///
    /// セッションレベルの advisory lock はトランザクションプーラ越しでは機能しない。
    /// ここを踏むと排他制御が「静かに」壊れるため、起動時に警告する。
    pub fn is_pooled_endpoint(&self) -> bool {
        host_of(&self.database_url).is_some_and(|h| h.contains("-pooler"))
    }
}

fn parse_env<T>(key: &str, default: T) -> anyhow::Result<T>
where
    T: FromStr,
    T::Err: std::fmt::Display,
{
    match std::env::var(key) {
        Ok(raw) => raw
            .parse::<T>()
            .map_err(|e| anyhow!("環境変数 {key} の値が不正です: {e}")),
        Err(_) => Ok(default),
    }
}

/// `scheme://user:password@host/...` のパスワード部分を伏せる。
///
/// パスワードに `@` が含まれうるので、ホストの直前の `@`（最後の `@`）で切る。
fn redact_url(raw: &str) -> String {
    let Some((scheme, rest)) = raw.split_once("://") else {
        return "***".to_string();
    };
    match rest.rsplit_once('@') {
        Some((userinfo, host)) => {
            let user = userinfo.split_once(':').map_or(userinfo, |(u, _)| u);
            format!("{scheme}://{user}:***@{host}")
        }
        // 認証情報を含まない URL はそのまま
        None => raw.to_string(),
    }
}

/// 接続文字列からホスト名だけを取り出す。
fn host_of(raw: &str) -> Option<&str> {
    let (_, rest) = raw.split_once("://")?;
    let after_userinfo = rest.rsplit_once('@').map_or(rest, |(_, h)| h);
    after_userinfo.split(['/', ':', '?']).next()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn パスワードを伏せる() {
        assert_eq!(
            redact_url("postgres://ops_hub:secret@db:5432/ops_hub"),
            "postgres://ops_hub:***@db:5432/ops_hub"
        );
    }

    #[test]
    fn パスワードに記号が含まれても伏せる() {
        assert_eq!(
            redact_url("postgres://u:p@ss/w0rd@host.example.com/db"),
            "postgres://u:***@host.example.com/db"
        );
    }

    #[test]
    fn 認証情報がなければそのまま() {
        assert_eq!(
            redact_url("postgres://localhost:5432/ops_hub"),
            "postgres://localhost:5432/ops_hub"
        );
    }

    #[test]
    fn スキームがなければ全体を伏せる() {
        assert_eq!(redact_url("ops_hub:secret@db"), "***");
    }

    #[test]
    fn ホスト名を取り出す() {
        assert_eq!(
            host_of("postgres://u:p@ep-cool-1.ap-southeast-1.aws.neon.tech/db?sslmode=require"),
            Some("ep-cool-1.ap-southeast-1.aws.neon.tech")
        );
        assert_eq!(host_of("postgres://db:5432/x"), Some("db"));
    }

    #[test]
    fn プール済みエンドポイントを検出する() {
        let pooled = Config {
            database_url: "postgres://u:p@ep-cool-1-pooler.aws.neon.tech/db".into(),
            port: 8080,
            run_lock_key: 8421337,
            db_max_connections: 5,
            db_acquire_timeout: Duration::from_secs(5),
        };
        assert!(pooled.is_pooled_endpoint());

        let direct = Config {
            database_url: "postgres://u:p@ep-cool-1.aws.neon.tech/db".into(),
            run_lock_key: 8421337,
            ..pooled.clone()
        };
        assert!(!direct.is_pooled_endpoint());
    }
}
