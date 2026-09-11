# ops-hub 実装ロードマップ

## 1. 正本

実装時の優先順位は次のとおり。

1. 要件定義 v0.2
2. 基本設計 v1.0
3. 詳細設計 v1.0
4. `docs/implementation-plan.md`
5. 各 `docs/task-ID-<name>.md`
6. ADR（`docs/adr/NNNN-*.md`）

`ID` は次のいずれかとする。

```text
legacy task:
01〜06

managed task:
Q1, Q2, Q3, ...
07, 08, ...
```

要件定義 v0.1 は旧版とし、差分確認以外では実装根拠にしない。

詳細設計で基本設計から明示的に修正された事項は、詳細設計を優先する。

タスク実装中に既存の設計判断と矛盾する変更が必要になった場合は、実装側で勝手に変更せず、対象タスクの `Spec Deviations` に記録して設計フェーズへ戻す。

---

## 2. 開発ワークフロー

managed task は Claude Code と Codex を次の責務で併用する。

managed task は次のIDを指す。

```text
Q1, Q2, Q3, ...
07, 08, ...
```

Task 01〜06 は legacy task とし、既存成果物を維持する。

```text
Claude Code

/spec ID
  ↓
設計・Acceptance Criteria・Design Decisions を確定
  ↓
/impl ID
  ↓
Codex 用実装プロンプトを生成
  ↓

Codex

実装・テスト・Implementation Record 更新
  ↓
make verify
  ↓

Claude Code

/review ID
  ↓
read-only review
  ├─ 問題あり → /fix ID → Codex 修正 → 再 review
  └─ READY
       ↓
人間が commit
```

Q系列は正式に次のコマンド形式を使用する。

```text
/spec Q1
/impl Q1
/review Q1
/fix Q1
```

数値系列の managed task も同じ形式を使用する。

```text
/spec 07
/impl 07
/review 07
/fix 07
```

通常の責務分担:

* Claude Code: 設計、仕様化、レビュー、障害解析
* Codex: 実装、テスト、レビュー指摘の修正
* 人間: 最終判断、commit、push、deploy

Claude Code と Codex は同じ working tree を同時編集しない。

---

## 3. タスク状態

managed task の `docs/task-ID-*.md` は YAML frontmatter で状態を管理する。

managed task:

```text
Q1, Q2, Q3, ...
07, 08, ...
```

Q系列の例:

```yaml
---
id: "Q1"
slug: ci-quality-gate
status: DRAFT
depends_on: ["06"]
---
```

数値系列の例:

```yaml
---
id: "07"
slug: stale-lock-recovery
status: DRAFT
depends_on: ["06", "Q3"]
---
```

filename の task ID と frontmatter の `id` は一致しなければならない。

例:

```text
docs/task-Q1-ci-quality-gate.md
        ↓
id: "Q1"
```

状態遷移と変更主体:

```text
DRAFT --Human only--> APPROVED
APPROVED --Codex only--> IMPLEMENTED
IMPLEMENTED --Claude only--> READY
READY --Human only--> DONE
```

`FIX`はstatusではなくreview verdictであり、`IMPLEMENTED`のままCodexへ戻す。設計判断・外部操作・安全境界で停止する場合は`BLOCKED`とする。未作成taskはtask docを持たず、index上だけ`NOT CREATED`と表示する。

Task 01〜06 は legacy task とし、既存形式の実装記録を維持する。

Task 01〜06 に対して frontmatter への一括書き換えは行わない。

---

## 4. ロードマップ

既存の機能タスク 01〜16 の番号は変更しない。

Q系列は、16機能タスクの番号体系を崩さずに追加する横断品質トラックとする。

```text
機能タスク
01 ─ 02 ─ 03 ─ 04 ─ 05 ─ 06 ─────────────── 07 ─ 08 ─ ... ─ 16
                              │                ↑
                              └ Q1 → Q2 → Q3 ─┘
```

Q系列の目的:

```text
Q1
CI / Quality Gate 基盤
  ↓
Q2
main branch protection
  ↓
Q3
Dependabot / dependency operations
  ↓
07以降の機能実装
```

Task 07 は機能上 Task 06 の続きである一方、開発順序上は Q3 完了後に開始する。

| ID | 状態      | タスク                                   | 主な完了条件                                                             | 依存     |
| -: | ------- | ------------------------------------- | ------------------------------------------------------------------ | ------ |
| 01 | done    | プロジェクト雛形・`/health`・compose・Dockerfile | `docker compose up --build -d` で API healthy、`GET /health` 疎通      | —      |
| 02 | done    | migration `0001`                      | ENUM / `targets` / `target_states` 作成、revert → run が成功             | 01     |
| 03 | done    | migration `0002`〜`0004`               | 全テーブル・制約・索引が設計どおり生成される                                             | 02     |
| 04 | done    | `AppError` / RFC 9457 Problem Details | SQLSTATE 分類テスト成功、5xx は `trace_id` のみ公開                             | 03     |
| 05 | done    | `RunLock`                             | 同一DBの2接続で advisory lock 競合を再現、release 後に再取得可能                      | 04     |
| 06 | done    | `POST /v1/runs` 骨格                    | `runs_test.rs` ①正常開始 202、②競合 200 `already_running` が green         | 05     |
| Q1 | next    | CI / Quality Gate 基盤                  | managed task対応、toolchain固定、`make verify`、CI、audit workflow、SHA pin | 06     |
| Q2 | planned | main branch protection                | Q1 の `verify` job を required check 候補として main を保護                  | Q1     |
| Q3 | planned | Dependabot / dependency operations    | dependency update 運用を追加し、CI / Ruleset と統合                          | Q2     |
| 07 | planned | 取り残し lock 回収 + `/v1/runs` 入口仕様の完成     | `runs_test.rs` ③ stale lock 回収が green。必要なら rate limit もここで閉じる      | 06, Q3 |
| 08 | planned | probe + masking                       | HTTP結果4分類、masking unit test 6件以上が green                            | 07     |
| 09 | planned | `domain/status`                       | 状態遷移の純粋関数テスト 7〜8件が green                                           | 08     |
| 10 | planned | `incident_service`                    | incident integration test 5件程度が green                              | 09     |
| 11 | planned | `Notifier` + Slack + `outbox_service` | outbox test 5件程度、Slack実送信、retry/failed遷移確認                         | 10     |
| 12 | planned | `POST /v1/notifications` + auth       | notifications test 5件程度、冪等性・認証境界が green                            | 11     |
| 13 | planned | 日次集計 + 90日削除                          | 手計算値と成功率一致、古い `checks` 削除確認                                        | 12     |
| 14 | planned | status page / Askama                  | 対象状態・失敗理由分類・応答成功率がブラウザで確認可能                                        | 13     |
| 15 | planned | GitHub Actions monitor                | 手動実行で 200/202、ops-hub 停止時 workflow failure + GitHub標準通知            | 14     |
| 16 | planned | OpenAPI + CI運用最終確認 + Runbook          | OpenAPI、Q1〜Q3、Task15 monitor、Runbook の最終整合確認                       | 15     |

---

## 5. Task 01 — プロジェクト雛形・`/health`・compose・Dockerfile

### ゴール

Rust/Axum/PostgreSQL を使った ops-hub の最小実行基盤を作り、ローカルDocker環境と本番用コンテナの土台を確立する。

### 主な実装

* Rust 1.96
* Axum 0.8
* PostgreSQL 17
* `GET /health`
* Clap CLI

  * `serve`
  * `healthcheck`
* Docker Compose
* multi-stage Dockerfile
* distroless non-root runtime
* host と container の `DATABASE_URL` を分離

### 完了条件

* `docker compose up --build -d` が成功する
* DBがhealthyになる
* APIが起動する
* `GET /health` が成功する
* distroless環境でCLI healthcheckが動作する

---

## 6. Task 02 — migration `0001`

### ゴール

監視対象と現在状態を保持する最小DBモデルを構築する。

### 主な対象

* ENUM
* `targets`
* `target_states`

### 設計上の重要点

設定と状態を分離する。

```text
targets
  └─ 監視設定

target_states
  └─ 現在状態
```

### 完了条件

* migration適用成功
* migration revert成功
* 再度migration適用成功
* constraint / index が意図どおり存在する

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

* `outbox.dedupe_key` UNIQUE
* 未解決 incident は1 target につき1件
* `events.idempotency_key` UNIQUE
* 必要な外部キー・CHECK制約
* 運用クエリに必要なindex

### 完了条件

* 0001〜0004 がfresh DBへ適用可能
* revert → run が可能
* テーブル・constraint・indexが設計どおり

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

* SQLSTATE分類テスト成功
* 4xx / 5xx のProblem Details形式が正しい
* 5xxで内部情報が漏れない

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

## 11. Q1 — CI / Quality Gate 基盤

### ゴール

Task 07以降の機能開発へ進む前に、ローカル検証、CI、AI workflow、task document の品質基盤を一つのルールへ統一する。

Q1 は純粋な品質・CI・AIワークフロー基盤タスクとする。

### 主な対象

* managed task の定義
* Q系列の task document validation
* Q系列の task index
* next task の動的判定
* Rust toolchain 固定
* `make verify`
* GitHub Actions CI
* dependency audit workflow
* GitHub Actions SHA pin
* Claude Code / Codex scaffold
* task template
* implementation plan / workflow documentation

### managed task

```text
managed task
=
Q系列
+
Task 07以降
```

次を正式にサポートする。

```text
/spec Q1
/impl Q1
/review Q1
/fix Q1
```

Task IDを数値に限定しない。

`int(task_id)` に依存する処理は helper に分離し、Q系列を整数変換しない。

分類:

```text
01〜06 → legacy
Q*    → managed
07+   → managed
```

### Acceptance Criteria

* [ ] AC-1: Q系列が `check_task_docs.py` の検証対象になる
* [ ] AC-2: Q系列が `task-INDEX.md` に表示され、next task が動的になる
* [ ] AC-3: Rust 1.96.0 / rustfmt / clippy が toolchain file で固定される
* [ ] AC-4: `make verify` が `DATABASE_URL` 無しの `SQLX_OFFLINE` build を検証する
* [ ] AC-5: PR と main push で CI / `verify` が起動する
* [ ] AC-6: CI PostgreSQL 17 に migration 適用後 `make verify` が成功する
* [ ] AC-7: CI が Neon / production secrets を一切使用しない
* [ ] AC-8: `cargo audit` が別workflowで weekly/manual 実行され、`verify` とは独立する
* [ ] AC-9: GitHub Actions の `uses:` がすべて full SHA pin される（Docker service image のtag pinは対象外）
* [ ] AC-10: `AGENTS.md` / `CLAUDE.md` / `.claude/skills/*/SKILL.md` の `NN` 表記が `ID` へ一般化される（`docs/templates/adr-template.md` の `NNNN` は対象外）
* [ ] AC-11: `docs/ai/PROJECT.md` の「現在地点」表がQ1〜Q3を含み、次タスクをQ1として記載する
* [ ] AC-12: `docs/ai/CI-POLICY.md` がQ1でコード品質CIを構築する方針と矛盾しないよう更新される

### Design Decisions / invariants

* DD-1: required候補job名は `verify` で固定する
* DD-2: `cargo audit` は `make verify` に含めない
* DD-3: Neon production DB をCI検証に使わない
* DD-4: `monitor.yml` と `ci.yml` を混ぜない
* DD-5: Q2以前に branch protection を設定しない
* DD-6: Q3以前に Dependabot を追加しない
* DD-7: Task 15 の monitor 実装を Q1 で先取りしない
* DD-8: 最終品質ゲートは引き続き `make verify` 一つとする
* DD-9: task IDの正本表記は大文字（`Q1`）。filename↔frontmatter比較は正規化してから行う
* DD-10: Rust toolchainバージョンの単一正本は `back_cargo/rust-toolchain.toml`。CI/Dockerfileへバージョン番号を重複hardcodeしない
* DD-11: `docs/templates/adr-template.md` の `NNNN` はADR番号でありQ1の一般化対象外
* DD-12: AC-9のSHA pin対象は `uses:` のActionsのみ。Docker service imageのtagはQ1 scope外
* DD-13: `docs/ai/CI-POLICY.md` の「monitor.ymlと品質CIを混ぜない」記述は維持し、「Task 16より前に先回りしない」記述だけを更新する
* DD-14: `docs/ai/PROJECT.md` の更新は「現在地点」表とその説明文に限定する

### task document validation

Q系列と Task 07以降を検証対象とする。

概念上の filename pattern:

```python
TASK_RE = re.compile(
    r"^task-(?P<id>Q[1-9]\d*|\d{2})-.*\.md$",
    re.IGNORECASE,
)
```

managed task では次も検証する。

```text
filename ID
    ==
frontmatter id
```

例:

```text
task-Q1-ci-quality-gate.md
        ↓
id: "Q1"
```

`status=done`、Acceptance Criteria、Spec Deviations、Review Record、`make verify` evidence の既存ルールは変更しない。

### task index

ROADMAP の順序を実行順の正本とする。

```text
01
 ↓
...
 ↓
06
 ↓
Q1
 ↓
Q2
 ↓
Q3
 ↓
07
 ↓
...
 ↓
16
```

`docs/task-INDEX.md` の「次タスク」は hard-code しない。

ROADMAP 上で最初の `status != done` の task を自動的に次タスクとする。

```text
Q1 done → Q2
Q2 done → Q3
Q3 done → 07
```

### Rust toolchain

Rust toolchain は repository で固定する。

最低限:

```text
Rust 1.96.0
rustfmt
clippy
```

`Cargo.toml` の `rust-version = "1.96"` と整合させる。

### make verify

最終品質ゲートは引き続き:

```bash
make verify
```

一つとする。

Rust build / check は `DATABASE_URL` を必要としない SQLx offline mode で検証できなければならない。

CI用 PostgreSQL が必要な integration test / migration verification は production DB と分離する。

### CI

PR と main push で品質CIを実行する。

required check候補となる job 名は:

```text
verify
```

で固定する。

CIでは PostgreSQL 17 service container を使用する。

fresh CI database に migration を適用したうえで `make verify` を成功させる。

CIで Neon production database を使用しない。

CIで production secrets を使用しない。

### cargo audit

`cargo audit` は `make verify` へ含めない。

dependency audit は別 workflow とし、少なくとも次をサポートする。

```text
weekly
workflow_dispatch
```

dependency advisory DB や外部要因によって通常の実装品質ゲートが不安定になることを避ける。

### GitHub Actions pin

GitHub Actions workflow の `uses:` は full commit SHA で固定する。

mutable tagのみで実行しない。

### AI scaffold

次の表現を managed task ベースへ統一する。

```text
Task 07以降
docs/task-NN-*.md
/spec NN
/impl NN
/review NN
/fix NN
```

変更後:

```text
managed task
docs/task-ID-*.md
/spec ID
/impl ID
/review ID
/fix ID
```

Claude skill の argument hint も `"NN"` から `"ID"` へ一般化する。

Q系列を正式例として記載する。

```text
/spec Q1
/impl Q1
/review Q1
/fix Q1
```

### task template

`docs/templates/task-template-v3.md` の `"NN"` は `"ID"` へ一般化する。

既存の次の構造は変更しない。

* YAML frontmatter
* Acceptance Criteria
* Design Decisions
* Spec Deviations
* Implementation Record
* Review Record

### Q1で変更しないもの

実コード本体には変更を入れない。

```text
back_cargo/src/**
migrations/**
tests/**
```

また、次はQ1で先取りしない。

```text
main branch protection / Ruleset → Q2
Dependabot                      → Q3
GitHub Actions monitor          → Task 15
```

既に要件を満たしている既存設定は不要に変更しない。

---

## 12. Q2 — main branch protection

### ゴール

Q1 で構築した品質ゲートを main branch の merge 条件として強制する。

### 前提

Q1 が `done` であること。

required check候補:

```text
verify
```

### 主な対象

* GitHub Ruleset / branch protection
* pull request 経由の変更
* required status check
* main branch の直接変更防止
* repository運用ルール

### 不変条件

Q2 では CI の品質ロジック自体を再実装しない。

品質判定は Q1 の `verify` job / `make verify` を利用する。

### 完了条件

* main branch 保護設定が有効
* required status check として `verify` を使用可能
* CI失敗時に保護された変更をmergeできない
* Q1のCIと設定が矛盾しない
* 設定内容をtask docへ記録

---

## 13. Q3 — Dependabot / dependency operations

### ゴール

dependency update を継続的に検出・検証できる運用へ移行する。

### 前提

Q2 が `done` であること。

### 主な対象

* Dependabot
* Rust dependency update
* frontend dependency update
* dependency PR運用
* CIとの統合

### 不変条件

Dependabot PR も通常の品質ゲートを通す。

```text
Dependabot PR
  ↓
verify
  ↓
Ruleset
  ↓
merge判断
```

Q3で通常CIへ dependency audit を混ぜない。

`cargo audit` の定期実行はQ1で構築した独立workflowを維持する。

### 完了条件

* Dependabot設定が有効
* Rust dependency update が検出可能
* frontend dependency update が検出可能
* Dependabot PR に `verify` が実行される
* Q2 Ruleset と整合する
* dependency更新運用をtask docへ記録

---

## 14. Task 07 — 取り残し lock 回収

### 状態

Q1 → Q2 → Q3 完了後に着手する次の機能タスク。

### ゴール

取り残された advisory lock / running run を安全に回収し、`POST /v1/runs` が自己復旧できるようにする。

### Task 06からの引き継ぎ

* `pg_locks` から対象 advisory lock のbackend PIDを特定する
* `classid` / `objid` を結合して64bit keyを復元する
* `objid = $1` のみでlockを探さない
* stale と判断した場合のみ `pg_terminate_backend`
* terminate後のlock再取得は1回だけ
* 回収成功時は `recovered: true`
* 回収不能・権限不足なら `already_running` へフォールバックする
* terminate失敗を500へ直結させない

### rate limit

既存ロードマップ上で `/v1/runs` rate limit の担当タスクが明示されていないため、Task 07のspec段階で含めるかを確定する。

### 完了条件

* `runs_test.rs` ③ stale lock recovery が green
* 通常のlock競合挙動を壊していない
* 回収できないケースが安全に `already_running` へ戻る
* `make verify` 成功

---

## 15. Task 08 — probe + masking

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

* wiremockで4分類を再現
* masking unit test 6件以上
* secretがDB/ログへ残らない

---

## 16. Task 09 — `domain/status`

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

## 17. Task 10 — `incident_service`

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

* incident作成
* down継続で重複作成しない
* recovery
* transition失敗時rollback
* concurrent executionへの防御

---

## 18. Task 11 — `Notifier` + Slack + `outbox_service`

### ゴール

通知をOutbox Patternで永続化し、Slackへ安全に配信する。

### retry

* 配信失敗は次回runで再試行
* 最大5回
* 5回失敗後は `failed`
* 未配信通知を失わない

### 完了条件

* outbox test 5件程度
* retry遷移
* duplicate防止
* Slack実送信確認

---

## 19. Task 12 — `POST /v1/notifications` + auth

### ゴール

他プロダクトから通知要求を安全に受け付ける。

### 冪等性

`idempotency_key` を使い、送信側retryを安全に受け入れる。

### 完了条件

* 正常受付
* unauthorized
* unknown token
* duplicate idempotency key
* validation error

---

## 20. Task 13 — 日次集計 + 90日削除

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

* 手計算と集計値一致
* 0件期間はNULL
* 90日超データ削除
* 当日再計算が冪等

---

## 21. Task 14 — status page / Askama

### ゴール

ブラウザで各監視対象の状態と応答成功率を確認できるstatus pageを提供する。

### 表示するもの

* target name
* current status
* failure reason classification
* response success rate
* 必要なincident情報

### 表示しないもの

* response body全文
* response header
* token
* DB情報
* Slack Webhook
* URLの秘匿部分
* 内部stack trace

---

## 22. Task 15 — GitHub Actions monitor

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

* cron: `7 * * * *`
* request timeout: 120秒
* 最大3 attempts
* retry間隔: 15秒
* `cancel-in-progress: false`

### 完了条件

* `workflow_dispatch` で正常時200/202
* ops-hub停止時にworkflow failure
* GitHub標準通知経路で失敗を検知できる

---

## 23. Task 16 — OpenAPI + CI運用最終確認 + Runbook

### ゴール

API契約、Q系列で構築した品質基盤、Task 15 の監視経路、運用手順を最終確認し、ops-hub の開発・運用基盤を完成させる。

Task 16 では CI、branch protection、Dependabot を新規構築しない。

### OpenAPI

```text
/v1/openapi.json
```

を公開し、実装済みAPI契約と一致させる。

### CI / repository quality gate 最終確認

次の基盤が引き続き現役であることを確認する。

```text
Q1
CI / make verify
  ↓
Q2
main branch protection / Ruleset
  ↓
Q3
Dependabot
  ↓
Task 15
GitHub Actions monitor
  ↓
Task 16
最終整合確認
```

最低限確認するもの:

* Q1 で構築した `verify` job が現役である
* Q1 の CI が `make verify` を品質ゲートとして使用している
* Q1 の dependency audit workflow が現役である
* Q2 で設定した main branch protection / Ruleset が現役である
* Q3 の Dependabot が現役である
* Task 15 の monitor workflow が現役である
* `ci.yml` と `monitor.yml` の責務が分離されたままである
* production secrets に依存しないCI設計が維持されている
* GitHub Actions の SHA pin 方針が維持されている

### Runbook

最低限:

* ops-hub起動確認
* `/health`
* CI / quality gate failure
* `make verify` failure
* main branch protection / Ruleset
* Dependabot PR
* dependency audit workflow failure
* monitor workflow失敗
* stale run / stale lock
* Slack配信失敗
* failed outbox
* DB connection error
* service token rotation
* Neon障害
* migration troubleshooting

### 完了条件

* `/v1/openapi.json` が実装済みAPI契約と一致する
* Q1 の `verify` job / `make verify` が現役
* Q2 の main branch protection / Ruleset が現役
* Q3 の Dependabot が現役
* Task 15 の monitor workflow が現役
* CI / monitor / dependency update の責務分離が維持されている
* 主要Runbookが完成している
* `make verify` が成功する

---

## 24. 共通実装ルール

各タスク完了時に以下を残す。

```text
docs/task-ID-<name>.md
docs/commits/task-ID-<name>.txt
```

`ID`:

```text
legacy:
01〜06

managed:
Q1, Q2, Q3, ...
07, 08, ...
```

legacy task 01〜06 は既存形式の成果物を維持する。

managed task の task 文書には以下を含める。

* YAML frontmatter
* Acceptance Criteria
* Design Decisions / invariants
* Spec Deviations
* Implementation Record
* Review Record

managed task の filename ID と frontmatter `id` は一致しなければならない。

### migration

* 適用済みmigrationを編集しない
* schema変更は新規migration
* 破壊的変更は段階的migrationを検討
* backfill方法をtask docへ記載
* SQLx metadataへ影響する場合は `.sqlx/` を更新

### production DB

Claude Code / Codexの検証でNeon productionを使用しない。

GitHub Actions CIでもNeon productionを使用しない。

ローカル標準例:

```bash
export DATABASE_URL="postgres://ops_hub:ops_hub@localhost:5433/ops_hub"
```

---

## 25. 共通検証

最終品質ゲートは常に:

```bash
make verify
```

のみ。

局所テストは実装中に使ってよいが、最終検証の代替にはしない。

`cargo audit` は dependency / security advisory の独立検証であり、`make verify` には含めない。

---

## 26. レビュー基準

通常レビューはClaude Code `reviewer`。

次は `reviewer-critical` / Opusへエスカレーションする。

* `migrations/`
* `.sqlx/`
* advisory lock
* transaction boundary
* concurrent execution
* retry
* idempotency
* incident state transition

Severity:

```text
BLOCKER
HIGH
MEDIUM
LOW
```

BLOCKER / HIGHが残っている場合はREADYにしない。

品質・CI基盤については次も重点確認する。

* `make verify` の単一品質ゲート原則
* SQLx offline verification
* production secret separation
* GitHub Actions SHA pin
* CI / monitor workflow responsibility separation
* Ruleset required check name
* Dependabot PR の通常CI経路

---

## 27. 完了定義

managed task で `status: DONE` にできる条件:

* CodexとHuman ImmediateのAcceptance Criteriaがすべて`[x]`
* unresolved Spec Deviations がない
* `make verify` が成功
* Implementation Recordに検証証拠がある
* BLOCKER/HIGHが0件
* Review verdictが`READY`
* Humanがcommit/pushを完了

Human Deferred ACはDue付きで未確認のまま残せる。indexへ件数を表示し、後日の確認を隠さない。

Task 01〜06 は legacy task として既存の完了状態を維持する。

---

## 28. 現在地

現在地は各managed taskのfrontmatterを正本とし、`make task-index`で生成する`docs/task-INDEX.md`だけに表示する。この計画書へ状態を重複記録しない。

実行順:

```text
01
 ↓
02
 ↓
03
 ↓
04
 ↓
05
 ↓
06
 ↓
Q1
 ↓
Q2
 ↓
Q3
 ↓
07
 ↓
08
 ↓
09
 ↓
10
 ↓
11
 ↓
12
 ↓
13
 ↓
14
 ↓
15
 ↓
16
```

次の作業:

```text
/spec Q1
```

Q1では、機能実装を変更せず、managed task対応、AI scaffold、Rust toolchain、`make verify`、CI、独立したdependency audit workflowを確立する。

Q1完了後は `task-INDEX.md` の動的next task判定により Q2 が次タスクになる。

```text
Q1 done → Q2
Q2 done → Q3
Q3 done → 07
```
