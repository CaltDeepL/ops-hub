//! 実行（run）の起動。`POST /v1/runs` の中身。
//!
//! ## 処理順（詳細設計 タスク06 1章）
//!
//! ```text
//! try_acquire
//!   ├─ None  → 最新 running の run_id を取得 → 200 already_running
//!   └─ Some  → INSERT runs (running) → spawn へ lock と run_id を move
//!              → 202 started を即返却
//!              → [background] complete_empty → lock.release()
//! ```
//!
//! ## 型で守っていること
//!
//! ロック競合は **エラーではない**（詳細設計 3.2、D-3）。409 を返すと GitHub
//! Actions 側が失敗扱いになり、デッドマンスイッチが誤発報する。そのため
//! [`AppError`](crate::error::AppError) にバリアントを足さず、成功型
//! [`RunOutcome`] の一分岐として表現する。ハンドラは `Started` → 202、
//! `AlreadyRunning` → 200 に写すだけになる。

use sqlx::PgPool;
use uuid::Uuid;

use crate::error::AppResult;
use crate::repository::run_repo;
use crate::run_lock::RunLock;
use crate::state::AppState;

/// 起動の結果。`AlreadyRunning` は**正常系**。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunOutcome {
    Started {
        run_id: Uuid,
    },
    /// `run_id` は「実行中の run を引けたか」。引けなければ `None`（T6-3）。
    AlreadyRunning {
        run_id: Option<Uuid>,
    },
}

/// 実行を起動する。ロックが取れなければ何もせずに戻る。
///
/// 202 を返してから背後で処理する（D-1）。`RunLock` は `tokio::spawn` した
/// タスクへ move し、処理の最後で解放する。**spawn 前に解放してはいけない。**
/// 解放してしまうと、返した 202 の裏で次の実行が重なって走る。
pub async fn start(state: &AppState) -> AppResult<RunOutcome> {
    let Some(lock) = RunLock::try_acquire(&state.db, state.config.run_lock_key).await? else {
        // 競合。実行中の run_id を引いて返す（引けなくても 200 は返す：T6-3）
        let run_id = run_repo::latest_running(&state.db).await?;
        tracing::info!(?run_id, "既に実行中のため起動を見送りました");
        return Ok(RunOutcome::AlreadyRunning { run_id });
    };

    // 記録用のコネクションはプールから別に取る。ロック保持コネクションは
    // `RunLock` の中にあり、`&mut PgConnection` を取り出す手段は無い（詳細設計 1.2）。
    let run_id = match run_repo::insert_running(&state.db).await {
        Ok(run_id) => run_id,
        Err(error) => {
            // ここで解放しないと、記録に失敗しただけでロックが次の実行まで残る
            release(lock).await;
            return Err(error.into());
        }
    };

    tokio::spawn(execute(state.db.clone(), lock, run_id));

    Ok(RunOutcome::Started { run_id })
}

/// バックグラウンドの本体。**この関数の中で `?` を使って早期 return しない。**
///
/// 途中で抜けると `runs` が `running` のまま残り、ロックの解放も飛ぶ。
///
/// タスク06の実処理は no-op。タスク08で `targets` の取得・probe・`checks` の
/// 記録がここに入り、`complete_empty` は実際の集計値を受け取る完了処理と、
/// 失敗時に `failed` へ倒す処理に置き換わる。
async fn execute(db: PgPool, lock: RunLock, run_id: Uuid) {
    tracing::info!(%run_id, "実行を開始しました（タスク06では対象の巡回を行いません）");

    // T6-1：no-op でも completed で閉じる。running のまま残すとタスク07の
    // 「直近15分の running」判定と実行履歴を汚す
    match run_repo::complete_empty(&db, run_id).await {
        Ok(true) => tracing::info!(%run_id, "実行を終了しました"),
        // タスク07のスイーパーが先に failed へ倒していた場合に来る。
        // 上書きはしていないので実害は無いが、遅延の証拠なので残す
        Ok(false) => {
            tracing::warn!(%run_id, "締めようとしたが既に running ではありませんでした");
        }
        Err(error) => tracing::error!(%run_id, error = ?error, "runs の締めに失敗しました"),
    }

    // T6-2：完了記録 → 解放の順。先に解放すると、次の run が開始できる一方で
    // 前の run がまだ running に見える窓ができ、DB上で running が2件になる
    release(lock).await;
}

/// ロックを解放する。失敗しても呼び出し側は続行する。
///
/// `release` が失敗するのはコネクションが既に壊れている場合で、そのときは
/// 切断によってサーバ側のロックも落ちる。`RunLock` の `Drop` も同じ経路を
/// 保険として持っている。
async fn release(lock: RunLock) {
    if let Err(error) = lock.release().await {
        tracing::warn!(error = ?error, "ロックの解放に失敗しました（切断による解放に委ねます）");
    }
}
