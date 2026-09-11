---

id: "Q1"
slug: ci-quality-gate
status: done
depends_on: ["06"]
---

# Task Q1 — CI / Quality Gate 基盤

## 1. 概要

| 項目       | 内容                                                                                      |
| -------- | --------------------------------------------------------------------------------------- |
| 上位ドキュメント | `docs/implementation-plan.md` §11「Q1 — CI / Quality Gate 基盤」                            |
| ゴール      | Task 07以降の機能開発に入る前に、managed task運用・ローカル検証・CI・依存監査・AI scaffold表記を一貫したルールへ統一する            |
| 完了条件     | AC-1〜AC-12をすべて満たし、GitHub Actions上で `verify` job が実際にgreenになる                            |
| 実装範囲外    | main branch protection / Ruleset（Q2）、Dependabot（Q3）、Task 15のmonitor実装、機能コード・migration変更 |

状態遷移:

```text
planned -> spec -> implementing -> review -> fixing -> review -> done

設計矛盾時:
blocked -> spec
```

---

## 2. 背景

Task 06完了時点で `POST /v1/runs` の骨格まで実装済み。

Q1着手前のscaffold導入によって、以下はすでに存在していた。

* `Makefile`

  * `verify`
  * `sqlx-prepare`
  * `task-index`
* `scripts/assert_local_database_url.py`
* `scripts/check_task_docs.py`
* `scripts/update_task_index.py`
* `docs/ai/PROJECT.md`
* `docs/ai/WORKFLOW.md`
* `docs/ai/CI-POLICY.md`
* `docs/templates/task-template-v3.md`
* `.claude/skills/{spec,impl,review,fix}/SKILL.md`
* `AGENTS.md`
* `CLAUDE.md`

Q1では既存差分を土台として使い、CI / managed task / AI scaffold間の不整合を解消した。

### Q1着手時に確認された主な問題

1. `check_task_docs.py`

   * filename側はcase-insensitiveだったが、frontmatter `id` との比較でcase正規化されていなかった。

2. Rust toolchain

   * `rust-toolchain.toml`
   * Dockerfile
   * GitHub Actions

   の3箇所にバージョン管理が分散していた。

3. `docs/ai/CI-POLICY.md`

   * 「Task 16より前に品質CIを作らない」という旧方針が残っていた。

4. `docs/ai/PROJECT.md`

   * Q1〜Q3追加後も「次はTask 07」のままだった。

5. AI scaffold

   * `NN`
   * `Task NN`

   という旧表記が残り、Q系列を正式に扱えなかった。

---

## 3. Acceptance Criteria

- [x] AC-1: Q系列を `check_task_docs.py` の検証対象にし、filename IDとfrontmatter `id` をcase-insensitiveで比較する
- [x] AC-2: Q系列を `task-INDEX.md` に表示し、next taskをstatusから動的算出する
- [x] AC-3: Rust 1.96.0 / rustfmt / clippyを `rust-toolchain.toml` で固定し、CIとDockerも整合させる
- [x] AC-4: `make verify` に `SQLX_OFFLINE=true cargo check --all-targets --all-features` を含める
- [x] AC-5: PR / main pushで `ci.yml` の `verify` jobを起動し、実際のgreen runを確認する
- [x] AC-6: PostgreSQL 17 service containerへmigration適用後、CI上で `make verify` を成功させる
- [x] AC-7: CIからNeon / production secretsを参照しない
- [x] AC-8: `cargo audit` を独立した `security-audit.yml` とし、weekly + manual実行にする
- [x] AC-9: GitHub Actionsの `uses:` を40桁full SHAで固定する
- [x] AC-10: AI scaffoldの `NN` / `Task NN` を `ID` / `managed task` へ一般化する
- [x] AC-11: `PROJECT.md` にQ1〜Q3と現在地点を反映する
- [x] AC-12: `CI-POLICY.md` をQ1の品質CI方針と整合させる

---

# 4. 設計判断

## CI / Quality Gate

### DD-1

required check候補のjob名は `verify` に固定する。

Q2のbranch protection / Rulesetもこの名前を参照する。

### DD-2

`cargo audit` は `make verify` に含めない。

dependency advisory DBなど外部要因によって通常の品質ゲートが不安定になることを避ける。

### DD-3

CIではNeon / production DBを使わない。

PostgreSQL 17 service containerのみを利用する。

### DD-4

以下を別workflowとして維持する。

```text
ci.yml
  -> コード品質CI

monitor.yml
  -> Task 15の運用監視
```

責務が異なるため統合しない。

### DD-5

Q2以前にbranch protection / Rulesetを設定しない。

### DD-6

Q3以前にDependabotを追加しない。

### DD-7

Task 15のmonitor実装をQ1で先取りしない。

### DD-8

最終品質ゲートは `make verify` 一つとする。

個別コマンドを「同等の検証」として代用しない。

---

## managed task

### DD-9

Task IDの正本表記は大文字とする。

```text
Q1
Q2
Q3
```

filename IDとfrontmatter `id` は双方を `.upper()` で正規化して比較する。

---

## Rust toolchain

### DD-10

Rust toolchainバージョンの単一正本は以下とする。

```text
back_cargo/rust-toolchain.toml
```

CIはこのファイルからtoolchainを自動解決する。

GitHub Actions側に次のようなバージョンhardcodeを持たせない。

```text
rustup toolchain install 1.96.0
```

Docker builderは同じchannelのexact tagを使用する。

```dockerfile
rust:1.96.0-slim-bookworm
```

部分バージョン指定は使用しない。

```dockerfile
rust:1.96-slim-bookworm
```

---

## AI scaffold / docs

### DD-11

`docs/templates/adr-template.md` の `NNNN` はADR番号なので変更対象外。

### DD-12

SHA pin対象はGitHub Actionsの `uses:` のみ。

```yaml
uses: ...
```

Docker service imageのtag pinはQ1のscope外。

### DD-13

`CI-POLICY.md` の以下の原則は維持する。

```text
品質CIとmonitor workflowを混ぜない
```

削除するのは旧方針:

```text
Task 16より前に品質CIを作らない
```

新方針:

```text
コード品質CIはQ1で確定する
```

### DD-14

`PROJECT.md` の変更対象は基本的に以下へ限定する。

* 現在地点
* Q1〜Q3
* 次タスク

他の技術スタックやTask 07 handoff等は変更しない。

---

# 5. 変更対象

## CI / toolchain

```text
Makefile
.github/workflows/ci.yml
.github/workflows/security-audit.yml
back_cargo/rust-toolchain.toml
back_cargo/Dockerfile
```

## managed task

```text
scripts/check_task_docs.py
scripts/update_task_index.py
docs/task-INDEX.md
docs/templates/task-template-v3.md
```

## AI scaffold

```text
AGENTS.md
CLAUDE.md

.claude/skills/spec/SKILL.md
.claude/skills/impl/SKILL.md
.claude/skills/review/SKILL.md
.claude/skills/fix/SKILL.md
```

## 方針文書

```text
docs/ai/PROJECT.md
docs/ai/WORKFLOW.md
docs/ai/CI-POLICY.md
docs/implementation-plan.md
```

## Task記録

```text
docs/task-Q1-ci-quality-gate.md
```

---

# 6. DB / APIへの影響

## DB

変更なし。

Q1では以下を変更しない。

```text
migrations/**
.sqlx/**
query
schema
```

## HTTP API

変更なし。

Q1はHTTP APIの契約・レスポンス・挙動には影響しない。

## ADR

追加なし。

Q1〜Q3を16機能タスクへ挿入する理由は、すでに `docs/implementation-plan.md` に記録されているため。

## Spec Deviations

- なし。

---

## 7. Implementation Record

_実装結果（Codexによる変更内容）を記録する。ローカル品質ゲート `make verify` の実行証跡は次節「8. Verification」を参照。_

## managed task

filename IDとfrontmatter `id` を双方 `.upper()` で正規化するよう修正した。

これにより、例えば以下も正しく一致する。

```text
docs/task-q1-xxx.md
id: "Q1"
```

case違いのfixtureを使ってPASSを確認済み。

---

## Task Index

next taskをstatusから動的に算出する。

以下をfixtureで確認済み。

```text
Q1 done -> Q2
Q2 done -> Q3
Q3 done -> 07
```

---

## Rust toolchain

単一正本:

```text
back_cargo/rust-toolchain.toml
```

設定:

```text
Rust 1.96.0
rustfmt
clippy
```

CI側のRustバージョンhardcodeを削除。

Docker builder:

```dockerfile
rust:1.96.0-slim-bookworm
```

---

## GitHub Actions SHA pin

2026-09-11時点で公式repositoryのtagを `git ls-remote` で照合。

対象:

```text
actions/checkout       v6
actions/setup-node     v6
Swatinem/rust-cache    v2.9.2
taiki-e/install-action v2.87.5
```

workflowの `uses:` はすべて40桁full commit SHAへ固定した。

---

## AI scaffold

以下を一般化した。

```text
NN
Task NN
```

↓

```text
ID
managed task
```

managed taskの定義:

```text
managed task
=
Q系列
+
Task 07以降
```

`AGENTS.md` についてはレビュー中にscope外のガバナンス追加が検出されたため削除し、最終的にはQ1で承認された変更だけを残した。

---

# 8. Verification

## ローカル品質ゲート

実行:

```bash
DATABASE_URL=postgres://ops_hub:ops_hub@localhost:5433/ops_hub make verify
```

結果:

```text
exit 0

SQLX_OFFLINE=true cargo check
  success

cargo fmt
  success

cargo clippy
  success

cargo sqlx prepare --check
  success

cargo test
  31 passed
  0 failed

npm lint
  success

npm build
  success

scripts/check_task_docs.py
  success
```

F1 / F1a / F1b修正後にも同じ検証を再実行し、成功を確認した。

---

# 9. GitHub Actions実行証跡

## Pull Request

```text
branch:
infrastructure/ci-setup

run:
34539577786

event:
pull_request

status:
completed

conclusion:
success
```

## main push

```text
run:
34539586345

event:
push

head_branch:
main

status:
completed

conclusion:
success
```

reviewerがGitHub Actions REST APIで独立確認した。

main push runでは `verify` job内で以下が成功していることも確認済み。

```text
PostgreSQL 17 service container起動
        ↓
Run migrations
        ↓
make verify
        ↓
success
```

したがってAC-5 / AC-6は完了。

---

# 10. Acceptance Evidence

| AC    | 主な証跡                                                               |
| ----- | ------------------------------------------------------------------ |
| AC-1  | `scripts/check_task_docs.py` + case違いfixture                       |
| AC-2  | `scripts/update_task_index.py` + Q1→Q2→Q3→07のfixture確認             |
| AC-3  | `rust-toolchain.toml` / `Cargo.toml` / Dockerfile / `rustc 1.96.0` |
| AC-4  | `Makefile` の `SQLX_OFFLINE=true cargo check`                       |
| AC-5  | Actions run `34539586345` / `34539577786`                          |
| AC-6  | run `34539586345` のmigration → verify成功                            |
| AC-7  | CI workflowで `secrets.*` 未使用                                       |
| AC-8  | `security-audit.yml` のschedule + workflow_dispatch                 |
| AC-9  | Actionsのfull SHA pinを `git ls-remote` で確認                          |
| AC-10 | `AGENTS.md` / `CLAUDE.md` / `.claude/skills/*`                     |
| AC-11 | `docs/ai/PROJECT.md`                                               |
| AC-12 | `docs/ai/CI-POLICY.md`                                             |

---

## 11. Review Record

_Claude Code親セッションがread-only reviewerの結果を転記する。_

- [x] `make verify` の実際の成功結果を確認した。
- [x] 全Acceptance Criteriaを具体的diff/test証拠へ対応付けた。
- [x] READY

### 最終結果

```text
BLOCKER  0
HIGH     0
MEDIUM   0
LOW      2
```

Disposition:

```text
READY
```

全AC完了。

未解決Spec Deviationsなし。

CI上で実際の `make verify` 成功を確認済み。

---

### Fix cycle 1

結果:

```text
BLOCKER 0
HIGH    3
MEDIUM  1
LOW     3

CHANGES REQUESTED
```

主な問題:

* F1: `AGENTS.md` にQ1 scope外のガバナンス追加
* F1a: `.env` 読み取りルールの矛盾
* F1b: `sed` の分類問題
* F2: CI green証跡未取得
* F4: AC-3とDD-10の文言矛盾

---

### Fix cycle 2

F1 / F1a / F1b / F4を解消。

残件:

```text
HIGH 1
  F2のみ
```

F2はコード不具合ではなく、

```text
実際にPR / push
       ↓
GitHub Actions実行
       ↓
green証跡取得
```

が必要だった。

---

### Fix cycle 3

PR #2を作成・merge。

```text
PR head:
8af9284

merge commit:
7418cbda2e80056394c9477643a79c4381c4ab42
```

reviewerがActions APIとlocal git双方からhead SHAを照合。

さらに `verify` jobの

```text
Run migrations
Verify
```

両stepの `conclusion=success` を確認した。

最終結果:

```text
BLOCKER 0
HIGH    0
MEDIUM  0
LOW     2

READY
```

---

# 12. Findings

## 解消済み

### F1 — HIGH → 解消

`AGENTS.md` に追加されていたQ1 scope外のガバナンスセクションを削除。

最終的な実質変更は以下のみ。

```text
docs/task-NN-*.md
    ↓
docs/task-ID-*.md
```

および

```text
Task 07以降
    ↓
managed task（Q系列およびTask 07以降）
```

---

### F1a — HIGH → 解消

scope外セクション削除に伴って `.env` 自動承認ルールも消滅。

---

### F1b — LOW → 解消

scope外セクション削除に伴い `sed` 分類問題も消滅。

---

### F2 — HIGH → 解消

PR / main push双方のGitHub Actions green証跡を取得。

```text
34539577786
34539586345
```

reviewerも独立検証済み。

---

### F4 — MEDIUM → 解消

AC-3とDD-10のRust Docker image pin方針の表現を統一。

実装方針はDD-10のまま。

```dockerfile
rust:1.96.0-slim-bookworm
```

---

# 13. 残っているLOW指摘

## F5 — Q2へ引き継ぎ

対象:

```text
.github/workflows/ci.yml
```

現在:

```yaml
cancel-in-progress: true
```

これが `push: main` にも適用される。

Q2で `verify` をrequired checkにする場合、mainへの連続pushによって進行中のrunがcancelされる可能性がある。

Q2のRuleset設計時にconcurrencyを見直す。

---

## F6 — 軽微

対象:

```text
scripts/update_task_index.py
back_cargo/rust-toolchain.toml
```

ファイル末尾に改行がない。

機能・CIへの影響はない。

---

# 14. Q2への引き継ぎ

Q2では以下を維持する。

### 1. required check名

```text
verify
```

job名を変更しない。

### 2. CI DB

```text
PostgreSQL 17 service container
```

Neon / production secretsは使用しない。

### 3. security audit

```text
security-audit.yml
```

はrequired checkにしない。

### 4. concurrency

F5をQ2の設計検討事項として扱う。

```yaml
cancel-in-progress: true
```

をmain pushにも適用し続けるべきか確認する。

---

# 15. 再現コマンド

```bash
export DATABASE_URL="postgres://ops_hub:ops_hub@localhost:5433/ops_hub"

make verify
```

---

# 16. 最終状態

```text
Q1
CI / Quality Gate 基盤

Acceptance Criteria:
12 / 12 complete

CI:
green

Review:
READY

Spec Deviations:
none

次タスク:
Q2 — Branch Protection / Ruleset
```
