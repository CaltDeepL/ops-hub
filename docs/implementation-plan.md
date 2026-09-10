# ops-hub 実装ロードマップ

## 1. 正本

実装時の優先順位は次のとおり。

1. 要件定義 v0.2
2. 基本設計 v1.0
3. 詳細設計 v1.0
4. `docs/implementation-plan.md`
5. 各 `docs/task-NN-<name>.md`
6. ADR（`docs/adr/NNNN-*.md`）

要件定義 v0.1 は旧版とし、差分確認以外では実装根拠にしない。

詳細設計で基本設計から明示的に修正された事項は、詳細設計を優先する。
タスク実装中に既存の設計判断と矛盾する変更が必要になった場合は、実装側で勝手に変更せず、対象タスクの `Spec Deviations` に記録して設計フェーズへ戻す。

---

## 2. 開発ワークフロー

Task 07 以降は Claude Code と Codex を次の責務で併用する。

```text
Claude Code
/spec NN
  ↓
設計・Acceptance Criteria・Design Decisions を確定
  ↓
/impl NN
  ↓
Codex 用実装プロンプトを生成
  ↓
Codex
実装・テスト・Implementation Record 更新
  ↓
make verify
  ↓
Claude Code
/review NN
  ↓
read-only review
  ├─ 問題あり → /fix NN → Codex 修正 → 再 review
  └─ READY
       ↓
人間が commit
```

通常の責務分担:

- Claude Code: 設計、仕様化、レビュー、障害解析
- Codex: 実装、テスト、レビュー指摘の修正
- 人間: 最終判断、commit、push、deploy

Claude Code と Codex は同じ working tree を同時編集しない。

---

## 3. タスク状態

Task 07 以降の `docs/task-NN-*.md` は YAML frontmatter で状態を管理する。

```yaml
---
id: "07"
slug: stale-lock-recovery
status: spec
depends_on: ["06"]
---
```

状態遷移:

```text
planned
  ↓
spec
  ↓
implementing
  ├── blocked → spec
  ↓
review
  ├── fixing → review
  └── done
```

定義:

- `planned`: 未着手
- `spec`: Claude Code による仕様確定済み
- `implementing`: Codex 実装中
- `blocked`: 設計変更・外部依存などで停止
- `review`: Claude Code レビュー中
- `fixing`: Codex によるレビュー指摘修正中
- `done`: Review Record が READY で、人間の commit が可能

Task 01〜06 は既存形式の実装記録を維持し、frontmatter への一括書き換えは行わない。

---

## 4. 16タスク一覧

| # | 状態 | タスク | 主な完了条件 | 依存 |
|---:|---|---|---|---|
| 01 | done | プロジェクト雛形・`/health`・compose・Dockerfile | `docker compose up --build -d` で API healthy、`GET /health` 疎通 | — |
| 02 | done | migration `0001` | ENUM / `targets` / `target_states` 作成、revert → run が成功 | 01 |
| 03 | done | migration `0002`〜`0004` | 全テーブル・制約・索引が設計どおり生成される | 02 |
| 04 | done | `AppError` / RFC 9457 Problem Details | SQLSTATE 分類テスト成功、5xx は `trace_id` のみ公開 | 03 |
| 05 | done | `RunLock` | 同一DBの2接続で advisory lock 競合を再現、release 後に再取得可能 | 04 |
| 06 | done | `POST /v1/runs` 骨格 | `runs_test.rs` ①正常開始 202、②競合 200 `already_running` が green | 05 |
| 07 | next | 取り残し lock 回収 + `/v1/runs` 入口仕様の完成 | `runs_test.rs` ③ stale lock 回収が green。必要なら rate limit もここで閉じる | 06 |
| 08 | planned | probe + masking | HTTP結果4分類、masking unit test 6件以上が green | 07 |
| 09 | planned | `domain/status` | 状態遷移の純粋関数テスト 7〜8件が green | 08 |
| 10 | planned | `incident_service` | incident integration test 5件程度が green | 09 |
| 11 | planned | `Notifier` + Slack + `outbox_service` | outbox test 5件程度、Slack実送信、retry/failed遷移確認 | 10 |
| 12 | planned | `POST /v1/notifications` + auth | notifications test 5件程度、冪等性・認証境界が green | 11 |
| 13 | planned | 日次集計 + 90日削除 | 手計算値と成功率一致、古い `checks` 削除確認 | 12 |
| 14 | planned | status page / Askama | 対象状態・失敗理由分類・応答成功率がブラウザで確認可能 | 13 |
| 15 | planned | GitHub Actions monitor | 手動実行で 200/202、ops-hub 停止時 workflow failure + GitHub標準通知 | 14 |
| 16 | planned | OpenAPI + CI + Runbook | `/v1/openapi.json`、`make verify` を呼ぶCI、主要Runbook完成 | 15 |

---

## 5. Task 01 — プロジェクト雛形・`/health`・compose・Dockerfile

### ゴール

Rust/Axum/PostgreSQL を使った ops-hub の最小実行基盤を作り、ローカルDocker環境と本番用コンテナの土台を確立する。

### 主な実装

- Rust 1.96
- Axum 0.8
- PostgreSQL 17
- `GET /health`
- Clap CLI
  - `serve`
  - `healthcheck`
- Docker Compose
- multi-stage Dockerfile
- distroless non-root runtime
- host と container の `DATABASE_URL` を分離

### 完了条件

- `docker compose up --build -d` が成功する
- DBがhealthyになる
- APIが起動する
- `GET /health` が成功する
- distroless環境でCLI healthcheckが動作する

---

## 6. Task 02 — migration `0001`

### ゴール

監視対象と現在状態を保持する最小DBモデルを構築する。

### 主な対象

- ENUM
- `targets`
- `target_states`

### 設計上の重要点

設定と状態を分離する。

```text
targets
  └─ 監視設定

target_states
  └─ 現在状態
```

### 完了条件

- migration適用成功
- migration revert成功
- 再度migration適用成功
- constraint / index が意図どおり存在する

---

## 7. Task 03 — migration `0002`〜`0004`

### ゴール

ops-hub の永続化モデルを完成させる。

### 想定テーブル

```text
runs
checks
check_dailies
incidents
events
outbox
```

### 特に確認する制約

- `outbox.dedupe_key` UNIQUE
- 未解決 incident は1 target につき1件
- `events.idempotency_key` UNIQUE
- 必要な外部キー・CHECK制約
- 運用クエリに必要なindex

### 完了条件

- 0001〜0004 がfresh DBへ適用可能
- revert → run が可能
- テーブル・constraint・indexが設計どおり

---

## 8. Task 04 — `AppError` / Problem Details

### ゴール

APIエラーをRFC 9457 Problem Detailsへ統一する。

### 主な実装

```text
AppError
  ↓
IntoResponse
  ↓
application/problem+json
```

PostgreSQL SQLSTATE をHTTP statusへ分類する。

### セキュリティ

5xxでは内部エラー詳細、SQL、DB情報、secretをレスポンスへ出さない。

公開する識別情報は原則 `trace_id` のみとする。

### 完了条件

- SQLSTATE分類テスト成功
- 4xx / 5xx のProblem Details形式が正しい
- 5xxで内部情報が漏れない

---

## 9. Task 05 — `RunLock`

### ゴール

`POST /v1/runs` の多重起動を PostgreSQL advisory lock で防止する。

### 完了条件

同一DBに2 connectionを張り、

1. connection A がlock取得
2. connection B は取得失敗
3. Aがrelease
4. Bが取得成功

を確認する。

### Task 07への重要な引き継ぎ

`pg_locks` の advisory lock key は `classid` / `objid` から64bit値を復元する。
`objid = $1` だけで判定する旧方式を使わない。

---

## 10. Task 06 — `POST /v1/runs` 骨格

### ゴール

advisory lock を入口にrun開始をDBへ記録し、HTTPは即座に返却する。

### 処理順

```text
POST /v1/runs
  ↓
RunLock::try_acquire
  ├─ 競合
  │    ↓
  │  最新 running run_id を検索
  │    ↓
  │  200 already_running
  │
  └─ lock取得
       ↓
     INSERT runs(status=running)
       ↓
     lock + run_id を tokio::spawn へ move
       ↓
     202 started
       ↓
     background
       ↓
     Task 06ではno-op runをcompletedへ更新
       ↓
     lock.release()
```

### 確定仕様

正常開始:

```http
HTTP/1.1 202 Accepted

{"run_id":"<uuid>","status":"started"}
```

競合:

```http
HTTP/1.1 200 OK

{"run_id":null,"status":"already_running"}
```

### 不変条件

```text
run completed
    ↓
unlock
```

unlockを先に実行しない。

---

## 11. Task 07 — 取り残し lock 回収

### 状態

次に着手するタスク。

### ゴール

取り残された advisory lock / running run を安全に回収し、`POST /v1/runs` が自己復旧できるようにする。

### Task 06からの引き継ぎ

- `pg_locks` から対象 advisory lock のbackend PIDを特定する
- `classid` / `objid` を結合して64bit keyを復元する
- `objid = $1` のみでlockを探さない
- stale と判断した場合のみ `pg_terminate_backend`
- terminate後のlock再取得は1回だけ
- 回収成功時は `recovered: true`
- 回収不能・権限不足なら `already_running` へフォールバックする
- terminate失敗を500へ直結させない

### rate limit

既存ロードマップ上で `/v1/runs` rate limit の担当タスクが明示されていないため、Task 07のspec段階で含めるかを確定する。

### 完了条件

- `runs_test.rs` ③ stale lock recovery が green
- 通常のlock競合挙動を壊していない
- 回収できないケースが安全に `already_running` へ戻る
- `make verify` 成功

---

## 12. Task 08 — probe + masking

### ゴール

監視対象へHTTP requestを実行し、安全なcheck結果へ変換する。

### HTTP分類

```text
success
timeout
http_error
connection_error
```

Probeはtrait化せず、reqwest実装をwiremockで直接検証する。

### 完了条件

- wiremockで4分類を再現
- masking unit test 6件以上
- secretがDB/ログへ残らない

---

## 13. Task 09 — `domain/status`

### ゴール

監視結果から状態遷移を決定するロジックを純粋関数として実装する。

### 原則

```text
previous state
+
observation
+
rule parameters
    ↓
next state / action
```

### 完了条件

純粋関数テスト7〜8件以上がgreen。

---

## 14. Task 10 — `incident_service`

### ゴール

status transition とincident永続化を一貫したtransactionで実行する。

### 重要な整合性

```text
BEGIN
  incidents INSERT / UPDATE
  target_states UPDATE
COMMIT
```

### 完了条件

- incident作成
- down継続で重複作成しない
- recovery
- transition失敗時rollback
- concurrent executionへの防御

---

## 15. Task 11 — `Notifier` + Slack + `outbox_service`

### ゴール

通知をOutbox Patternで永続化し、Slackへ安全に配信する。

### retry

- 配信失敗は次回runで再試行
- 最大5回
- 5回失敗後は `failed`
- 未配信通知を失わない

### 完了条件

- outbox test 5件程度
- retry遷移
- duplicate防止
- Slack実送信確認

---

## 16. Task 12 — `POST /v1/notifications` + auth

### ゴール

他プロダクトから通知要求を安全に受け付ける。

### 冪等性

`idempotency_key` を使い、送信側retryを安全に受け入れる。

### 完了条件

- 正常受付
- unauthorized
- unknown token
- duplicate idempotency key
- validation error

---

## 17. Task 13 — 日次集計 + 90日削除

### ゴール

長期保持用の日次統計を作り、raw checkデータを適切に削除する。

### 成功率

```sql
CASE
  WHEN sum(total_count) = 0 THEN NULL
  ELSE round(
    sum(success_count)::numeric
    / sum(total_count) * 100,
    2
  )
END
```

### raw retention

```sql
DELETE FROM checks
WHERE started_at < now() - interval '90 days';
```

### 完了条件

- 手計算と集計値一致
- 0件期間はNULL
- 90日超データ削除
- 当日再計算が冪等

---

## 18. Task 14 — status page / Askama

### ゴール

ブラウザで各監視対象の状態と応答成功率を確認できるstatus pageを提供する。

### 表示するもの

- target name
- current status
- failure reason classification
- response success rate
- 必要なincident情報

### 表示しないもの

- response body全文
- response header
- token
- DB情報
- Slack Webhook
- URLの秘匿部分
- 内部stack trace

---

## 19. Task 15 — GitHub Actions monitor

### ゴール

ops-hub自身の停止を、ops-hub/Slackとは独立した経路で検出する。

### 基本動作

```text
cron
  ↓
POST /v1/runs
  ↓
200 / 202 → success
other / timeout → failure
```

### 想定設定

- cron: `7 * * * *`
- request timeout: 120秒
- 最大3 attempts
- retry間隔: 15秒
- `cancel-in-progress: false`

### 完了条件

- `workflow_dispatch` で正常時200/202
- ops-hub停止時にworkflow failure
- GitHub標準通知経路で失敗を検知できる

---

## 20. Task 16 — OpenAPI + CI + Runbook

### ゴール

API契約・品質ゲート・運用手順を完成させる。

### OpenAPI

```text
/v1/openapi.json
```

### CI

品質ゲートは:

```bash
make verify
```

のみを呼ぶ。

### Runbook

最低限:

- ops-hub起動確認
- `/health`
- monitor workflow失敗
- stale run / stale lock
- Slack配信失敗
- failed outbox
- DB connection error
- service token rotation
- Neon障害
- migration troubleshooting

---

## 21. 共通実装ルール

各タスク完了時に以下を残す。

```text
docs/task-NN-<name>.md
docs/commits/task-NN-<name>.txt
```

Task 07以降はtask文書に以下も含める。

- YAML frontmatter
- Acceptance Criteria
- Design Decisions / invariants
- Spec Deviations
- Implementation Record
- Review Record

### migration

- 適用済みmigrationを編集しない
- schema変更は新規migration
- 破壊的変更は段階的migrationを検討
- backfill方法をtask docへ記載
- SQLx metadataへ影響する場合は `.sqlx/` を更新

### production DB

Claude Code / Codexの検証でNeon productionを使用しない。

ローカル標準例:

```bash
export DATABASE_URL="postgres://ops_hub:ops_hub@localhost:5433/ops_hub"
```

---

## 22. 共通検証

最終品質ゲートは常に:

```bash
make verify
```

のみ。

局所テストは実装中に使ってよいが、最終検証の代替にはしない。

---

## 23. レビュー基準

通常レビューはClaude Code `reviewer`。

次は `reviewer-critical` / Opusへエスカレーションする。

- `migrations/`
- `.sqlx/`
- advisory lock
- transaction boundary
- concurrent execution
- retry
- idempotency
- incident state transition

Severity:

```text
BLOCKER
HIGH
MEDIUM
LOW
```

BLOCKER / HIGHが残っている場合はREADYにしない。

---

## 24. 完了定義

Task 07以降で `status: done` にできる条件:

- Acceptance Criteria がすべて `[x]`
- unresolved Spec Deviations がない
- `make verify` が成功
- Implementation Recordに検証証拠がある
- Review RecordのVerificationが完了
- BLOCKER/HIGHが0件
- Review disposition が `READY`

この時点で人間がcommitする。

---

## 25. 現在地

```text
01  done
02  done
03  done
04  done
05  done
06  done
07  NEXT
08  planned
09  planned
10  planned
11  planned
12  planned
13  planned
14  planned
15  planned
16  planned
```

次の作業:

```text
/spec 07
```

Task 07では、Task 06から引き継いだ stale advisory lock 回収仕様と `/v1/runs` rate limit の担当範囲を最初に確定する。
