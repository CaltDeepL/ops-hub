//! 実行（run）の起動。`POST /v1/runs` の中身。
//!
//! ## 型で守っていること
//!
//! ロック競合は **エラーではない**（詳細設計 3.2、D-3）。409 を返すと GitHub
//! Actions 側が失敗扱いになり、デッドマンスイッチが誤発報する。そのため
//! [`AppError`](crate::error::AppError) にバリアントを足さず、成功型
//! [`RunOutcome`] の一分岐として表現する。ハンドラは `Started` → 202、
//! `AlreadyRunning` → 200 に写すだけになる。
//!
//! ## タスク6の範囲
//!
//! 「ロックを取り、`runs` に1行作り、202 を返し、背後で締める」までを通す。
//!
//! ## タスク8で足したもの
//!
//! [`perform`] に巡回の中身を入れた。`targets` の取得 → probe → `checks` の記録。
//! 状態遷移（`target_states`）・インシデント・通知はタスク9以降。
//!
//! ## タスク7で足したもの
//!
//! [`start`] の先頭で [`recovery::sweep_stale_runs`] を呼ぶ。常駐スケジューラを
//! 持たない構成なので、「定期的に回る処理」を置ける場所がここしかない。

use anyhow::Context as _;
use tokio::task::JoinSet;
use uuid::Uuid;

use crate::error::AppResult;
use crate::provider::probe::{self, ProbeOutcome};
use crate::recovery;
use crate::repository::target_repo::Target;
use crate::repository::{check_repo, run_repo, target_repo};
use crate::run_lock::RunLock;
use crate::state::AppState;

/// `runs.error` に書く長さの上限（文字数）。
///
/// DB側に長さ制約は無いが、ステータス画面と Slack 通知にそのまま載りうるので、
/// アプリ側で詰める。
const ERROR_MAX_CHARS: usize = 1000;

/// 起動の結果。`AlreadyRunning` は**正常系**。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunOutcome {
    Started {
        run_id: Uuid,
    },
    /// `run_id` は「実行中の run を引けたか」。直前に終わっていれば `None`。
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
    // ロックを取る前に掃く。常駐スケジューラを持たない構成（基本設計の実行モデル）
    // では、ここが定期的に回る唯一の場所になる。
    //
    // 失敗しても起動は続ける。掃除は観測用の記録を整えるだけで、排他とは無関係
    // （ロックはセッション終了でサーバ側が解放する）。掃けなかったせいで
    // 本来の巡回まで止まる方が損失が大きい。
    if let Err(error) =
        recovery::sweep_stale_runs(&state.db, state.config.run_stale_after_secs).await
    {
        tracing::warn!(error = ?error, "取り残された run の回収に失敗しました（起動は継続します）");
    }

    let Some(lock) = RunLock::try_acquire(&state.db, state.config.run_lock_key).await? else {
        // 競合。実行中の run_id を引いて返す（引けなくても 200 は返す）
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

    tokio::spawn(execute(state.clone(), lock, run_id));

    Ok(RunOutcome::Started { run_id })
}

/// バックグラウンドの本体。**この関数から先で `?` を使って早期 return しない。**
///
/// 途中で抜けると `runs` が `running` のまま残り、ロックの解放も飛ぶ。
/// 失敗しうる処理は [`perform`] に閉じ込め、締めと解放は必ず通る位置に置く。
async fn execute(state: AppState, lock: RunLock, run_id: Uuid) {
    let db = &state.db;
    let result = perform(&state, run_id).await;

    let finished = match &result {
        Ok(targets_checked) => run_repo::finish_completed(db, run_id, *targets_checked).await,
        Err(error) => {
            tracing::error!(%run_id, error = ?error, "実行が失敗しました");
            run_repo::finish_failed(db, run_id, &truncate(&format!("{error:#}"))).await
        }
    };

    match finished {
        // タスク7のスイーパーが先に failed へ倒していた場合に来る。
        // 上書きはしていないので実害は無いが、10分以上掛かった証拠なので残す
        Ok(false) => tracing::warn!(%run_id, "締めようとしたが既に running ではありませんでした"),
        Ok(true) => tracing::info!(%run_id, "実行を終了しました"),
        Err(error) => tracing::error!(%run_id, error = ?error, "runs の締めに失敗しました"),
    }

    // 解放は最後。ここまでの失敗で早期 return しないこと
    release(lock).await;
}

/// 1巡の実処理。成功時は `checks` に記録できた件数を返す。
///
/// `Err` を返すのは「対象一覧を引けなかった」場合だけ。対象が落ちているのは
/// 正常系なので probe は値で結果を返し、`checks` の INSERT が1件失敗しても
/// その件を諦めて巡回を続ける。戻り値は記録できた件数なので、
/// `runs.targets_checked` と対象数がずれていれば記録漏れがあったと分かる。
///
/// ## DB コネクションを追加で消費しない
///
/// spawn するタスクは probe（HTTP）だけを行い、`checks` の INSERT は結果を
/// 回収するこのループが逐次に実行する。プール既定は5本で、うち1本は
/// `RunLock` が握ったまま。probe ごとにコネクションを取ると簡単に枯れる。
async fn perform(state: &AppState, run_id: Uuid) -> anyhow::Result<i32> {
    let targets = target_repo::list_enabled(&state.db)
        .await
        .context("監視対象の取得に失敗しました")?;
    let total = targets.len();
    tracing::info!(%run_id, total, "巡回を開始します");

    let concurrency = state.config.probe_concurrency;
    let mut queue = targets.into_iter();
    let mut running: JoinSet<(Target, ProbeOutcome)> = JoinSet::new();
    let mut recorded: i32 = 0;

    loop {
        while running.len() < concurrency
            && let Some(target) = queue.next()
        {
            let client = state.http.clone();
            running.spawn(async move {
                let outcome = probe::probe(&client, &target).await;
                (target, outcome)
            });
        }

        let Some(joined) = running.join_next().await else {
            break;
        };

        let (target, outcome) = match joined {
            Ok(pair) => pair,
            // probe は panic しない作りだが、万一の panic で1巡全体を落とさない
            Err(error) => {
                tracing::error!(%run_id, error = ?error, "probe タスクが異常終了しました");
                continue;
            }
        };

        tracing::info!(
            %run_id,
            target = %target.name,
            result = outcome.result.as_str(),
            duration_ms = outcome.duration_ms,
            status_code = ?outcome.status_code,
            degraded = outcome.degraded,
            "probe の結果"
        );

        match check_repo::insert(&state.db, run_id, target.id, &outcome).await {
            Ok(()) => recorded += 1,
            Err(error) => tracing::warn!(
                %run_id,
                target = %target.name,
                error = ?error,
                "checks の記録に失敗しました（巡回は継続します）"
            ),
        }
    }

    if usize::try_from(recorded).ok() != Some(total) {
        tracing::warn!(%run_id, total, recorded, "記録できなかった対象があります");
    }

    Ok(recorded)
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

/// 文字数で詰める。マルチバイトを途中で切らないよう `chars()` で数える。
fn truncate(message: &str) -> String {
    if message.chars().count() <= ERROR_MAX_CHARS {
        return message.to_owned();
    }
    message.chars().take(ERROR_MAX_CHARS).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 短いメッセージはそのまま() {
        assert_eq!(truncate("失敗しました"), "失敗しました");
    }

    #[test]
    fn 長いメッセージを文字数で詰める() {
        let long = "あ".repeat(ERROR_MAX_CHARS + 10);
        let truncated = truncate(&long);
        assert_eq!(truncated.chars().count(), ERROR_MAX_CHARS);
        // バイト境界で切っていたらここで panic している
        assert!(truncated.ends_with('あ'));
    }
}
