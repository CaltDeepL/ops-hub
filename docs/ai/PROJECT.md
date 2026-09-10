# ops-hub — AI共通プロジェクトコンテキスト

この文書はClaude CodeとCodexが共有する「現在状態の短い入口」。
長期的な正本・経緯を置き換えない。

## 正本の優先順位

既存 `docs/implementation-plan.md` に従う。

1. 要件定義 v0.2
2. 基本設計 v1.0
3. 詳細設計 v1.0
4. 要件定義 v0.1 は旧版

詳細設計が基本設計を明示的に修正している場合、その箇所は詳細設計を優先する。

## 現在地点

16タスクのうち Task 01〜06 を完了済みとして扱い、次は Task 07。

| Task | 内容 | 状態 |
|---:|---|---|
| 01 | プロジェクト雛形・`/health`・compose・Dockerfile | done |
| 02 | migration 0001 | done |
| 03 | migration 0002〜0004 | done |
| 04 | AppError / RFC 9457 problem+json | done |
| 05 | RunLock / advisory lock | done |
| 06 | `POST /v1/runs` 骨格 | done |
| 07 | 取り残し lock 回収 + run入口仕様の仕上げ | next |

Task 08以降は `docs/implementation-plan.md` の順序を維持する。

## 現在の技術スタック

- Rust 1.96
- Axum 0.8
- PostgreSQL 17
- SQLx（ORMなし、SQLを直接記述）
- Docker / Docker Compose
- distroless runtime
- Clap CLI: `serve` / `healthcheck`
- 本番DB: Neon
- 本番ホスティング: Renderを想定した設計

## 現在の実装境界

Task 06時点の `POST /v1/runs`:

```text
POST /v1/runs
  ↓
RunLock::try_acquire
  ├─ None
  │    ↓
  │  最新 running run_id
  │    ↓
  │  200 already_running
  │
  └─ Some(lock)
       ↓
     INSERT runs(status=running)
       ↓
     lock + run_id を tokio::spawn へ move
       ↓
     202 started を即返却
       ↓ background
     Task06ではno-op runをcompletedへ
       ↓
     advisory lock release
```

不変条件:

- completed記録 → unlock の順序。
- lock取得とrun INSERTは原子的ではないため、競合レスポンスの `run_id: null` を許容する。
- `AppState` は Task 06 で `db` と `config: Arc<Config>` を持つ形へ統一済み。
- Task 07回収成功時だけ `recovered: true`。

## Task 07 handoff

Task 06から確定している事項:

- stale/running判定は直近15分の `runs` を前提にする設計。
- `pg_locks` の advisory-lock key は `classid` / `objid` を結合して64bit値を復元する。
- 旧案の `objid = $1` のみの検索は禁止。
- stale backend を `pg_terminate_backend` した後のlock再取得は1回だけ。
- 回収できない場合・権限不足は `AlreadyRunning` へフォールバック。
- 回収成功は `RunOutcome::Started { recovered: true, .. }`。
- `runs_test.rs` のrate-limitケースはTask 07へ含め、`POST /v1/runs` の入口仕様を閉じる。

## システム設計の長期原則

- ops-hubは常駐内部schedulerを持たず、定期処理は `POST /v1/runs` を契機にする。
- 将来1 runで target check、状態遷移、outbox配信、日次集計を統括する。
- プロダクト → ops-hub の依存を一方向にし、ops-hubは個別プロダクトの業務知識を持たない。
- ops-hub障害がプロダクト機能停止を引き起こさない。
- retry/overlap/crash/timeoutを通常ケースとして扱う。

## 予定レイヤ

```text
handler      HTTP I/O / validation
service      use case orchestration
repository   PostgreSQL access
provider     HTTP probe / notification / clock boundaries
 domain      deterministic state rules
```

将来のAppStateはNotifier/Clockを持つ設計だが、現在taskより先回りして追加しない。

ProbeはHTTP下位挙動をwiremockで検証するため、既存設計ではtrait化しない。
NotifierとClockは将来trait境界を持つ。

## 開発DB

agentによる開発・SQLx prepare・testはローカル/CI DBのみ。

標準ローカル例:

```text
postgres://ops_hub:ops_hub@localhost:5433/ops_hub
```

Neon本番接続はagent作業では禁止。
