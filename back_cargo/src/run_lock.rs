//! `POST /v1/runs` の排他制御（PostgreSQL のセッションレベル advisory lock）。
//!
//! 詳細設計 1.2 の「ロック保持コネクションで他のクエリを流さない」を型で担保する。
//! 具体的には [`PoolConnection`] を [`RunLock`] に move し、`&mut PgConnection` を
//! 取り出す手段を一切公開しない。チェック結果の記録などは、呼び出し側が
//! プールから別のコネクションを取って行う。
//!
//! なぜそこまでするのか：バックエンドがクエリを実行している最中はクライアントの
//! 切断を検知できない。ロック保持コネクションで長いクエリを流すと、プロセスが
//! 消えてもロックが残り、次回以降の実行が延々とブロックされる。
//!
//! ## ロック競合は正常系
//!
//! 取得失敗は [`AppError`](crate::error::AppError) にしない（詳細設計 3.2）。
//! `Ok(None)` で返し、ハンドラ側は 200 + `already_running` を返す。409 にすると
//! GitHub Actions 側が失敗扱いになり、デッドマンスイッチが誤発報するため（D-3）。

use sqlx::pool::PoolConnection;
use sqlx::{PgPool, Postgres};

/// 実行ロック。保持している間、同じデータベースの他のセッションは同じキーを取れない。
///
/// 解放は [`release`](Self::release) を明示的に呼ぶ。`self` を消費するので、
/// 二重解放はコンパイルエラーになる。呼び忘れた場合は [`Drop`] が保険として働く
/// （後述の「drop 時の挙動」を参照）。
pub struct RunLock {
    /// ロック保持専用のコネクション。**ここから外へは絶対に出さない。**
    ///
    /// `Option` にしているのは `Drop` の中で所有権ごと取り出すため。
    /// `release()` 済みなら `None` になっている。
    conn: Option<PoolConnection<Postgres>>,
    key: i64,
}

impl RunLock {
    /// ロックの取得を1回だけ試みる。ブロックはしない。
    ///
    /// - `Ok(Some(lock))` … 取得できた
    /// - `Ok(None)` … 他のセッションが保持している（**正常系**）
    /// - `Err(_)` … DBに到達できない等。プール枯渇は `PoolTimedOut` で来る
    pub async fn try_acquire(pool: &PgPool, key: i64) -> Result<Option<Self>, sqlx::Error> {
        let mut conn = pool.acquire().await?;

        let acquired: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1)")
            .bind(key)
            .fetch_one(&mut *conn)
            .await?;

        if !acquired {
            // 取れなかったコネクションはロックを持っていないので、素直にプールへ返す。
            drop(conn);
            tracing::info!(key, "advisory lock は他のセッションが保持中です");
            return Ok(None);
        }

        tracing::debug!(key, "advisory lock を取得しました");
        Ok(Some(Self {
            conn: Some(conn),
            key,
        }))
    }

    /// 保持しているロックキー。
    pub fn key(&self) -> i64 {
        self.key
    }

    /// ロックを解放し、コネクションをプールへ返す。
    ///
    /// 解放後のコネクションはロックを持っていないので、通常のコネクションとして
    /// 再利用されて構わない。
    ///
    /// エラーを返した場合でも、コネクションが死んでいるならセッション終了に伴って
    /// ロックはサーバ側で解放される。呼び出し側はログに残すだけでよい。
    pub async fn release(mut self) -> Result<(), sqlx::Error> {
        // `self` を消費しているので、ここに来る時点で必ず `Some`。
        let Some(mut conn) = self.conn.take() else {
            return Ok(());
        };

        let released: bool = sqlx::query_scalar("SELECT pg_advisory_unlock($1)")
            .bind(self.key)
            .fetch_one(&mut *conn)
            .await?;

        if !released {
            // このセッションが持っていないロックを解放しようとした。
            // `try_acquire` を通っている限り起きないので、起きたらロジックの不整合。
            tracing::error!(
                key = self.key,
                "保持していない advisory lock を解放しようとしました"
            );
        } else {
            tracing::debug!(key = self.key, "advisory lock を解放しました");
        }

        Ok(())
    }
}

impl Drop for RunLock {
    /// `release()` を呼ばずに落ちた場合（`?` による早期 return、パニック、
    /// タスクのキャンセル）の保険。
    ///
    /// **プールへ返してはいけない。** `PoolConnection` を普通に drop すると
    /// セッションは生きたままアイドル接続としてプールに戻る。advisory lock は
    /// セッションスコープなので解放されず、しかもそのコネクションが再利用されると
    /// 「同一セッションでの再取得」が成功してしまい、`try_acquire` が嘘をつく
    /// （同一セッションからの取得はカウンタが増えるだけで必ず true になる）。
    ///
    /// `Drop` では await できないため、`detach()` でプール管理から外し、
    /// ソケットを閉じてセッションごと終わらせる。切断をサーバが検知した時点で
    /// ロックが解放される。プールの接続が1本減るが、これは異常系の経路なので
    /// 再接続のコストより確実性を取る。
    fn drop(&mut self) {
        let Some(conn) = self.conn.take() else {
            return;
        };

        tracing::warn!(
            key = self.key,
            "RunLock が release されずに drop されました。コネクションを切断してロックを解放します"
        );

        // detach でプールから切り離し、その場で閉じる。
        drop(conn.detach());
    }
}
