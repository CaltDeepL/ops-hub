//! 取り残しの回収（基本設計 3.4 / 詳細設計 2.3）。
//!
//! 扱う「取り残し」は2種類ある。原因も回収方法も違うので、混ぜない。
//!
//! ## 1. 取り残された `runs` 行（スイーパー）
//!
//! プロセスが巡回の途中で消えると、`runs` に `running` の行が残る。これは
//! 観測用の記録が実態とずれているだけで、**排他は壊れていない**（ロックは
//! セッション終了と同時にサーバ側で解放される）。ステータス画面と
//! デッドマンスイッチの判断材料が汚れるので、一定時間で `failed` に倒す。
//!
//! ## 2. 取り残された advisory lock（脱出ハッチ）
//!
//! 通常は起きない。advisory lock はセッションスコープなので、プロセスが
//! 死ねば TCP が切れ、サーバが検知した時点で解放される。
//!
//! 問題になるのは**セッションが生きているのに実行が進んでいない**場合。
//! ネットワークが片側だけ死ぬと、サーバは TCP keepalive がタイムアウトする
//! まで（既定で2時間超）セッションを生かし続ける。その間ロックは保持され、
//! 以降の `POST /v1/runs` は全て `already_running` を返し続ける。
//!
//! 自動では回収しない。「応答しないセッションを殺す」判断は、生きている
//! 実行を撃つ危険と背中合わせなので、人間が [`main`] の `unlock`
//! サブコマンド経由で明示的に行う。ここが提供するのは
//! 「誰が握っているか」を見る [`lock_holder`] と、
//! 「殺す」[`terminate_holder`] の2つだけ。
//!
//! [`main`]: https://github.com/CaltDeepL/ops-hub

use sqlx::{PgPool, Row as _};
use uuid::Uuid;

/// スイーパーが `runs.error` に書く文言。
///
/// 人間が見て「アプリが記録した失敗」と区別できるようにしておく。この文字列で
/// grep できることを運用上の前提にしているので、気軽に変えない。
pub const SWEPT_ERROR: &str =
    "実行が完了しないまま放置されたため、スイーパーが失敗として締めました";

/// advisory lock を握っているセッションの情報。
#[derive(Debug, Clone, PartialEq)]
pub struct LockHolder {
    /// PostgreSQL のバックエンド PID。`terminate_holder` に渡す。
    pub pid: i32,
    /// `active` / `idle` / `idle in transaction` など。
    ///
    /// **`idle` なら要注意**。ロックを持ったまま何もしていない＝取り残しの疑い。
    /// 正常に巡回中なら `active`、もしくはクエリの合間の一瞬だけ `idle` になる。
    pub state: Option<String>,
    /// `state` が最後に変わってからの経過秒数。
    ///
    /// `timestamptz` をそのまま返すと chrono を直接の依存に足すことになるので、
    /// SQL 側で秒数に落としてから受け取る。
    pub idle_secs: Option<f64>,
    /// 接続元。Render から張った接続か、手元から繋いだ psql かの区別に使う。
    pub client_addr: Option<String>,
}

/// 64bit の advisory lock キーを `pg_locks` の `(classid, objid)` へ分解する。
///
/// `pg_locks` は 64bit キーを 32bit 2つに割って持つ。上位が `classid`、
/// 下位が `objid`、`objsubid` は 1（2つの int で取った場合は 2）。
///
/// **算術シフトの符号拡張に注意**。キーが負だと `key >> 32` は上位が 1 で
/// 埋まり、`oid`（符号なし32bit）へのキャストで値が化ける。
/// [`Config::from_env`](crate::config::Config::from_env) が
/// `RUN_LOCK_KEY > 0` を入口で強制しているのはこのため。ここでも
/// マスクを掛けて二重に守る。
pub fn lock_key_parts(key: i64) -> (i64, i64) {
    ((key >> 32) & 0xFFFF_FFFF, key & 0xFFFF_FFFF)
}

/// `running` のまま `stale_after_secs` 秒を超えた行を `failed` に倒す。
///
/// 倒した行の ID を返す（0件なら空）。**実行中のロックには一切触らない。**
/// 生きている実行の行を倒してしまう可能性はあるが、それは想定内：
/// [`finish_completed`](crate::repository::run_repo::finish_completed) が
/// `WHERE status = 'running'` を持っているので、後から来た本来の締めが
/// この `failed` を上書きすることはない。10分を超えた事実の方が記録として正しい。
///
/// 閾値は秒で受ける。`make_interval(secs => ...)` は `double precision` を
/// 取るので、Rust 側も `f64` で通す。
pub async fn sweep_stale_runs(db: &PgPool, stale_after_secs: f64) -> sqlx::Result<Vec<Uuid>> {
    let swept = sqlx::query_scalar!(
        r#"
        UPDATE runs
        SET status      = 'failed',
            finished_at = now(),
            error       = COALESCE(error, $2)
        WHERE status = 'running'
          AND started_at < now() - make_interval(secs => $1::double precision)
        RETURNING id
        "#,
        stale_after_secs,
        SWEPT_ERROR
    )
    .fetch_all(db)
    .await?;

    if !swept.is_empty() {
        tracing::warn!(
            count = swept.len(),
            ?swept,
            stale_after_secs,
            "取り残された run を failed に倒しました"
        );
    }

    Ok(swept)
}

/// 指定キーの advisory lock を握っているセッションを引く。
///
/// 握られていなければ `None`。マクロ（`query!`）ではなく実行時クエリを使うのは、
/// `pg_locks` / `pg_stat_activity` のスキーマを `.sqlx` に焼き込みたくないため。
/// システムカタログの列はサーバのメジャーバージョンで変わりうるので、
/// オフラインメタデータの再生成をローカルの PostgreSQL バージョンに
/// 縛られる形にしたくない。
pub async fn lock_holder(db: &PgPool, key: i64) -> sqlx::Result<Option<LockHolder>> {
    let (classid, objid) = lock_key_parts(key);

    let row = sqlx::query(
        r#"
        SELECT a.pid,
               a.state,
               a.client_addr::text AS client_addr,
               EXTRACT(EPOCH FROM (now() - a.state_change))::double precision AS idle_secs
        FROM pg_locks l
        JOIN pg_stat_activity a ON a.pid = l.pid
        WHERE l.locktype = 'advisory'
          AND l.classid  = $1::bigint::oid
          AND l.objid    = $2::bigint::oid
          AND l.objsubid = 1
          AND l.granted
        LIMIT 1
        "#,
    )
    .bind(classid)
    .bind(objid)
    .fetch_optional(db)
    .await?;

    Ok(row.map(|row| LockHolder {
        pid: row.get("pid"),
        state: row.get("state"),
        idle_secs: row.get("idle_secs"),
        client_addr: row.get("client_addr"),
    }))
}

/// 指定 PID のバックエンドを落とす。**人間が明示的に呼ぶ経路からのみ使う。**
///
/// 落とすとそのセッションが持つ advisory lock は解放される。`true` が返れば
/// シグナルの送信に成功した（＝対象が存在した）。既に消えていれば `false`。
///
/// 巡回中のセッションを撃つと、その実行は途中で切れる。`runs` の行は
/// `running` のまま残るが、[`sweep_stale_runs`] が拾うので放置してよい。
pub async fn terminate_holder(db: &PgPool, pid: i32) -> sqlx::Result<bool> {
    let terminated: bool = sqlx::query_scalar("SELECT pg_terminate_backend($1)")
        .bind(pid)
        .fetch_one(db)
        .await?;

    if terminated {
        tracing::warn!(pid, "advisory lock を保持するセッションを切断しました");
    } else {
        tracing::info!(pid, "対象のセッションは既に存在しませんでした");
    }

    Ok(terminated)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 既定のキーを分解する() {
        // 32bit に収まるキーなので上位は 0、下位にそのまま出る
        assert_eq!(lock_key_parts(8_421_337), (0, 8_421_337));
    }

    #[test]
    fn 上位ビットのあるキーを分解する() {
        let key = (7_i64 << 32) | 12_345;
        assert_eq!(lock_key_parts(key), (7, 12_345));
    }

    #[test]
    fn 下位32ビットが符号付きの範囲を超えても化けない() {
        // 0x8000_0000 は i32 にすると負。oid は符号なしなので、
        // ここで負の値を作ってしまうと SQL 側の比較が外れる
        let key = 0x8000_0000_i64;
        let (classid, objid) = lock_key_parts(key);
        assert_eq!(classid, 0);
        assert_eq!(objid, 2_147_483_648);
        assert!(objid > 0, "oid へ渡す値が負になっている");
    }

    #[test]
    fn 負のキーでも上位ビットが埋まらない() {
        // Config が入口で弾くので通常は到達しないが、マスクの保険が効くこと
        let (classid, objid) = lock_key_parts(-1);
        assert_eq!(classid, 0xFFFF_FFFF);
        assert_eq!(objid, 0xFFFF_FFFF);
        assert!(classid >= 0 && objid >= 0);
    }
}
