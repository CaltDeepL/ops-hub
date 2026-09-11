# concurrency / run coordination レビューチェックリスト（非推奨・通常導線外）

共通review基準は`AGENTS.md`とtask docに統合済み。この文書は過去参照の互換用。

対象: advisory lock、run起動、retry、timeout、transaction、重複排除。

## 全般

- [ ] concurrent callerのwinner/loser挙動が明示されている。
- [ ] check-then-write raceをDB constraint/lock/atomic statement等で防いでいる。
- [ ] transaction境界が不変条件の全更新を含んでいる。
- [ ] timeoutで結果不明になった後のretryが重複副作用を起こさない。
- [ ] crash/restart途中状態から回復可能である。
- [ ] duplicate invocationのHTTP semanticsが仕様とtestで固定されている。

## ops-hub RunLock固定事項

- [ ] advisory lockを取得したconnectionの寿命がlock寿命と一致している。
- [ ] `completed` 記録より先にunlockしていない。
- [ ] lock競合は `200 already_running`。根拠なしに409へ変えていない。
- [ ] lock競合時 `run_id: null` raceを許容している。

## Task 07

- [ ] stale判断がtaskで定義したrunning時間条件に一致する。
- [ ] `pg_locks` key復元は `classid` / `objid` の両方を使う。
- [ ] `objid = RUN_LOCK_KEY` だけの旧SQLを使っていない。
- [ ] terminate対象backendを誤認しない条件がある。
- [ ] `pg_terminate_backend` 後の再取得は1回だけ。
- [ ] terminate失敗/権限不足/再取得失敗は安全に `AlreadyRunning` へ落ちる。
- [ ] 回収成功時だけ `recovered: true`。
- [ ] rate limitがdead-manの正常再試行まで不必要に壊さないことをtask仕様と照合した。
