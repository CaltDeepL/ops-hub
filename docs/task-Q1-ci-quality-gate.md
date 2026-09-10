---
id: "Q1"
slug: ci-quality-gate
status: fixing
# planned -> spec -> implementing -> review -> fixing -> review -> done
# 設計矛盾時: blocked -> spec
depends_on: ["06"]
---

# タスクQ1：CI / Quality Gate 基盤

| 項目 | 内容 |
|---|---|
| 上位ドキュメント | `docs/implementation-plan.md` §11「Q1 — CI / Quality Gate 基盤」 |
| ゴール | Task 07以降の機能開発に入る前に、managed task運用・ローカル検証・CI・依存監査・AI scaffold表記を1つの一貫したルールへ統一する |
| 完了条件 | AC-1〜AC-12 がすべて満たされ、CI上で `verify` job が実際にgreenになる（`make verify` のローカル実行だけでは完了扱いにしない） |
| 実装範囲外 | `back_cargo/src/**` / `migrations/**` / `tests/**`（機能実装）、main branch protection/Ruleset（Q2）、Dependabot（Q3）、GitHub Actions monitor（Task 15） |

## 1. Context / 現状

Task 06完了時点で `POST /v1/runs` の骨格は実装済み（`docs/task-INDEX.md` 上 01〜06は `done`）。

Q1着手前の状態として、直前のscaffold導入コミット（`task-6 claud code codex実装`）で次がすでに存在する。

- `Makefile`（`verify` / `sqlx-prepare` / `task-index` ターゲット）
- `scripts/assert_local_database_url.py` / `check_task_docs.py` / `update_task_index.py`
- `docs/ai/PROJECT.md` / `WORKFLOW.md` / `CI-POLICY.md` / `checklists/*`
- `docs/templates/task-template-v3.md` ほか
- `.claude/skills/{spec,impl,review,fix}/SKILL.md`、`AGENTS.md`、`CLAUDE.md`

この `/spec Q1` 起票時点で、**working treeには未コミットの変更がすでに存在する**（`git status` で確認可能）。

```text
変更: Makefile, back_cargo/Dockerfile, docs/implementation-plan.md,
      docs/task-INDEX.md, scripts/check_task_docs.py, scripts/update_task_index.py
新規: .github/workflows/ci.yml, .github/workflows/security-audit.yml,
      back_cargo/rust-toolchain.toml
```

architect による検証の結果、これらはAC-1〜AC-9の大半を実質的に満たす内容になっているが、次の問題が見つかっている。

- `scripts/check_task_docs.py` の `TASK_RE` は `re.IGNORECASE` だが、frontmatter `id` との比較が大文字小文字を正規化していない（`docs/task-q1-*.md` + `id: "Q1"` のような組み合わせで誤ってFAILする）。
- `back_cargo/Dockerfile` が `rust-toolchain.toml` をCOPYする一方、ベースイメージは `rust:1.96-slim-bookworm`（部分バージョン指定）のままで、`.github/workflows/ci.yml` 側は `rustup toolchain install 1.96.0` を個別にhardcodeしている。toolchainのバージョンが3箇所に分散し、正本が曖昧。
- `docs/ai/CI-POLICY.md` に「Task 16より前に品質CI workflowを先回りして作らない」という記述が残っており、Q1でコード品質CIを構築するという本設計と矛盾する。
- `docs/ai/PROJECT.md` の「現在地点」表・本文が「次はTask 07」のままで、Q1〜Q3を反映していない。
- AI scaffold（`AGENTS.md` / `CLAUDE.md` / `.claude/skills/*/SKILL.md` の `NN` → `ID` 一般化）はまだ未着手。

本taskは、これらの既存差分を**そのまま実装の土台として使いつつ**、上記の不整合を解消し、CI上での実際のgreen実行まで確認することをゴールとする。CI/deployment領域の設計判断を含むため、本spec作成時にarchitect（read-only）でAC/DD候補を検証済み。

## 2. Acceptance Criteria

- [x] AC-1: Q系列 (`Q1`, `Q2`, ...) が `scripts/check_task_docs.py` の検証対象になり、`task-Q1-ci-quality-gate.md` に対して実際にPASSする（大文字小文字を問わず filename ID と frontmatter `id` が一致すればPASSする）
- [x] AC-2: Q系列が `docs/task-INDEX.md` に表示され、`make task-index` 実行後の next task が `status` に応じて動的に変わる（`Q1 done → Q2`, `Q2 done → Q3`, `Q3 done → 07` を手動でfrontmatterを書き換えて再現確認する）
- [x] AC-3: Rust `1.96.0` / rustfmt / clippy が `back_cargo/rust-toolchain.toml` で固定され、`Cargo.toml` の `rust-version = "1.96"` と矛盾しない。CIワークフローはこのファイルから自動解決させ、`rustup toolchain install <version>` のようなバージョン番号のhardcodeを重複させない（DD-10）。Dockerfileのbuilderベースイメージは `rust-toolchain.toml` の `channel` と一致する具体バージョンタグ（例: `rust:1.96.0-slim-bookworm`）に固定する（DD-10）
- [x] AC-4: `make verify` が `DATABASE_URL` 無しの `SQLX_OFFLINE=true cargo check --all-targets --all-features` を含み、これがローカルで実際に成功する
- [ ] AC-5: PRと `main` push で `.github/workflows/ci.yml` の `verify` jobが起動する（実際にGitHub Actions上で1回green実行された証跡をImplementation Recordへ残す）
- [ ] AC-6: CIのPostgreSQL 17 service containerへmigration適用後、`make verify` がCI上で成功する
- [x] AC-7: CIがNeon / production secretsを一切参照しない（`secrets.*` 未使用、`DATABASE_URL` はservice containerのみ）
- [x] AC-8: `cargo audit` が `.github/workflows/security-audit.yml` として独立し、`schedule` (weekly) と `workflow_dispatch` の両方をサポートし、`make verify` には含まれない
- [x] AC-9: `.github/workflows/*.yml` の `uses:` がすべて40桁のfull commit SHAで固定され、コメントのバージョン番号と実際のSHAが一致する（Docker service image (`postgres:17-bookworm` 等) のタグ指定はこのACの対象外とする）
- [x] AC-10: `AGENTS.md` / `CLAUDE.md` / `.claude/skills/{spec,impl,review,fix}/SKILL.md` の `NN` / `Task NN` 表記が `ID` / `managed task` へ一般化される（`docs/templates/adr-template.md` の `NNNN` はADR番号のため対象外）
- [x] AC-11: `docs/ai/PROJECT.md` の「現在地点」表と本文がQ1〜Q3を含み、次タスクをQ1として記載する
- [x] AC-12: `docs/ai/CI-POLICY.md` が本designと矛盾しないよう更新される（「Task 16より前に品質CIを先回りしない」旨の記述を削除し、Q1でコード品質CIを確定する旨を明記する。「`monitor.yml` と品質CIを混ぜない」という方針は維持する）

## 3. 設計判断・不変条件

- DD-1: required候補job名は `verify` で固定する（Q2のbranch protectionが参照する名前と一致させる）
- DD-2: `cargo audit` は `make verify` に含めない。dependency advisory DBの外部要因で通常の品質ゲートを不安定にしない
- DD-3: Neon production DBをCI検証に使わない。CIは常にlocal PostgreSQL 17 service containerのみを使う
- DD-4: `monitor.yml`（Task 15の運用監視）と `ci.yml`（コード品質CI）を混ぜない。責務が異なるworkflowを1ファイルに統合しない
- DD-5: Q2以前にbranch protection/Rulesetを設定しない
- DD-6: Q3以前にDependabotを追加しない
- DD-7: Task 15のmonitor実装をQ1で先取りしない
- DD-8: 最終品質ゲートは引き続き `make verify` 一つとする。個別コマンドを「同等」として代用しない
- DD-9: task IDの正本表記は大文字（`Q1`）。`check_task_docs.py` の filename↔frontmatter比較は両者を同じ正規化（`.upper()`）を通してから比較する。現状の大文字小文字不一致バグ（`docs/task-q1-*.md` のようなlowercase filenameで誤FAIL/誤PASSしうる非対称性）を修正する
- DD-10: Rust toolchainバージョンの単一正本は `back_cargo/rust-toolchain.toml` とする。`.github/workflows/ci.yml` はこのファイルから自動解決させ、`rustup toolchain install <version>` のようなバージョン番号のhardcodeをワークフロー側に重複させない。`back_cargo/Dockerfile` のbuilderベースイメージは `rust-toolchain.toml` の `channel` と一致する具体バージョンタグ（例: `rust:1.96.0-slim-bookworm`）に固定し、`rust:1.96-slim-bookworm` のような部分バージョン指定を使わない
- DD-11: `docs/templates/adr-template.md` の `NNNN` はADR番号であり、Q1の `NN` → `ID` 一般化の対象に含めない
- DD-12: AC-9のSHA pin対象は `uses:` で参照するGitHub Actionsのみとする。Docker service container image（`postgres:17-bookworm` 等）のtag pinはQ1のscope外とし、別途判断が必要になった場合はSpec Deviationsで扱う
- DD-13: `docs/ai/CI-POLICY.md` の「2種類のworkflowを混ぜない」（DD-4相当）という記述は維持したまま更新する。「Task 16より前に品質CIを先回りして作らない」という記述だけを削除し、「コード品質CIはQ1で確定する」と明記する
- DD-14: `docs/ai/PROJECT.md` の更新は「現在地点」表とその直後の説明文に限定し、他のセクション（技術スタック、実装境界、Task 07 handoffなど）の内容は変更しない

## 4. 想定変更箇所

- `docs/task-Q1-ci-quality-gate.md` — 本task doc（既に作成）
- `.github/workflows/ci.yml` — `verify` job。toolchain hardcode除去（DD-10）
- `.github/workflows/security-audit.yml` — 既存内容で概ねAC-8を満たす。変更なしの可能性が高い
- `back_cargo/rust-toolchain.toml` — 既存内容で概ねAC-3を満たす。変更なしの可能性が高い
- `back_cargo/Dockerfile` — builderベースイメージのタグをexact pinへ変更（DD-10）
- `Makefile` — 既存の `SQLX_OFFLINE` check追加で概ねAC-4を満たす。変更なしの可能性が高い
- `scripts/check_task_docs.py` — DD-9のcase正規化バグ修正
- `scripts/update_task_index.py` — 既存内容で概ねAC-1/AC-2を満たす。変更なしの可能性が高い
- `AGENTS.md` — `NN` → `ID` 一般化（AC-10）
- `CLAUDE.md` — `NN` → `ID` 一般化（AC-10）
- `.claude/skills/spec/SKILL.md` / `impl/SKILL.md` / `review/SKILL.md` / `fix/SKILL.md` — `argument-hint` と本文の `NN` → `ID` 一般化（AC-10）
- `docs/ai/PROJECT.md` — 現在地点表の更新（AC-11）
- `docs/ai/CI-POLICY.md` — Task 16前提の記述更新（AC-12）
- `docs/task-INDEX.md` — `make task-index` の生成物。手動編集しない

ファイル名の微調整は許容するが、DD-* を変える必要が出た場合は Spec Deviations を使う。

## 5. DB / migration / SQLx

変更なし。Q1は `migrations/` / `.sqlx/` を一切変更しない。

## 6. API / 互換性

変更なし。Q1はHTTP APIの挙動・契約に影響しない。

## 7. ADR

なし。Q系列（Q1〜Q3）を16機能タスクの番号体系を崩さずに挿入するという設計自体の理由は、既に `docs/implementation-plan.md` §1・§4 に記載済みであり、追加のADRを必要とするほどの独立した長期判断はQ1には無い。

## 8. Spec Deviations

- なし。

## 9. 検証

反復用の狭い検証:

```bash
export DATABASE_URL="postgres://ops_hub:ops_hub@localhost:5433/ops_hub"
cd back_cargo && env -u DATABASE_URL SQLX_OFFLINE=true cargo check --all-targets --all-features
python3 scripts/check_task_docs.py
python3 scripts/update_task_index.py && git diff --stat docs/task-INDEX.md
```

AC-5/AC-6はローカルだけで確認できないため、実際にPR/pushをトリガーしてGitHub Actions上で `verify` jobがgreenになったことを確認し、実行URLか run IDをImplementation Recordへ残す。

最終品質ゲート:

```bash
make verify
```

## 10. Implementation Record

_Codexが実装完了時に更新する。_

### 変更ファイル

- 品質ゲート / toolchain: `Makefile`, `back_cargo/rust-toolchain.toml`, `back_cargo/Dockerfile`
- GitHub Actions: `.github/workflows/ci.yml`, `.github/workflows/security-audit.yml`
- managed task運用: `scripts/check_task_docs.py`, `scripts/update_task_index.py`, `docs/task-INDEX.md`, `docs/templates/task-template-v3.md`
- AI scaffold / 方針文書: `AGENTS.md`, `CLAUDE.md`, `.claude/skills/{spec,impl,review,fix}/SKILL.md`, `docs/ai/PROJECT.md`, `docs/ai/WORKFLOW.md`, `docs/ai/CI-POLICY.md`, `docs/implementation-plan.md`
- 実装記録: `docs/task-Q1-ci-quality-gate.md`

### 実装上の判断

- Q1着手前から存在した未コミット差分を土台として維持し、DD-1〜DD-14に必要な箇所だけを補完した。
- filename IDとfrontmatter `id` は双方を `.upper()` で正規化して比較する。小文字filenameの一時fixtureでPASSを確認した。
- CIのRust手動installを削除し、`back_cargo` 配下のCargo実行が `rust-toolchain.toml` を自動解決する構成にした。Docker builderは同じchannelのexact tag `1.96.0` に固定した。
- task indexのnext taskは一時fixtureのfrontmatterを順に変更し、`Q1 done → Q2`, `Q2 done → Q3`, `Q3 done → 07` を確認した。
- ActionsのSHAは2026-09-11に公式repositoryのtagを `git ls-remote` で照合した（checkout v6、setup-node v6、rust-cache v2.9.2、install-action v2.87.5）。
- F1/F1a/F1b対応として、`AGENTS.md` からスコープ外の新規セクション `自律動作・自動承認`、`最小差分原則`、`実装時の基本フロー`、`Codexがしてはいけない判断`、`完了条件` を削除した。既存セクションへの実質的な追加も削除し、AC-10で承認された `task-NN` → `task-ID` と「Task 07以降」→「managed task（Q系列およびTask 07以降）」の2箇所だけを差分として残した。

### DB / APIへの影響

- 変更なし。migration / query / `.sqlx/` / HTTP APIは変更していない。

### Verification evidence

```text
DATABASE_URL=postgres://ops_hub:ops_hub@localhost:5433/ops_hub make verify
exit 0
- SQLX_OFFLINE=true cargo check: success
- cargo fmt / clippy / sqlx prepare --check: success
- cargo test: 31 passed, 0 failed
- npm lint / build: success
- scripts/check_task_docs.py: success
```

F1/F1a/F1b修正後の再検証（2026-09-11）:

```text
DATABASE_URL=postgres://ops_hub:ops_hub@localhost:5433/ops_hub make verify
exit 0
- SQLX_OFFLINE=true cargo check: success
- cargo fmt / clippy / sqlx prepare --check: success
- cargo test: 31 passed, 0 failed
- npm lint / build: success
- scripts/check_task_docs.py: success
```

CI実行証跡（AC-5/AC-6）:

```text
未取得。2026-09-11時点のGitHub Actions APIはworkflow run 0件。
本taskでは指示どおりcommit/pushを行っていないため、AC-5/AC-6は未完了。
```

### 残課題

- 人間が本差分をPRまたはmainへpushして `verify` jobを起動し、green runのURLまたはrun IDを本recordへ追記する。確認後にAC-5/AC-6を完了へ更新する。

## 11. Review Record

_Claude Code親セッションがread-only reviewerの結果を転記する。_

### Verification

- [ ] `make verify` の実際の成功結果を確認した。（1〜2巡目ともにローカルDocker PostgreSQLが使えず、フル`make verify`は未実行。SQLX_OFFLINE checkとcheck_task_docs.pyの部分実行のみ確認済み。CIでの実行証跡もまだ無い＝AC-5/6未達と表裏）
- [x] 全Acceptance Criteriaを具体的diff/test証拠へ対応付けた。（1巡目: reviewer + reviewer-criticalの2段階、2巡目: reviewerによるfix確認、で file:line 根拠を確認済み）

### Fix cycle 2（F1/F1a/F1b/F4対応後の再レビュー）

`reviewer`（Sonnet）が `git diff -- AGENTS.md` を実測: 差分は `docs/task-NN-*.md`→`docs/task-ID-*.md`（AGENTS.md:15）と「Task 07以降は」→「managed task（Q系列およびTask 07以降）は」（AGENTS.md:118）の2箇所のみに縮小されており、F1/F1a/F1bの原因だった新規ガバナンスセクション（自律動作・自動承認等）は完全に削除されたことを確認。F4はtask doc側の文言修正で解消済み。AGENTS.md以外への無関係な変更混入もなし。AC-1〜AC-4, AC-7〜AC-12は今回のfixで壊れていないことも再確認済み。ローカル `SQLX_OFFLINE cargo check` と `check_task_docs.py` は成功。フル`make verify`はローカルDB無しのため未実行（1巡目と同じ制約）。

残る唯一の指摘はF2（AC-5/AC-6未達）で、これはCodexの実装欠陥ではなく、人間がpush/PRしてCI上の`verify` job green実行証跡を得る必要がある構造的な残課題。BLOCKER 0 / HIGH 1（F2のみ）/ MEDIUM 0 / LOW 2（F5, F6、いずれも今回対応不要）。

### Acceptance evidence

| Criterion | Evidence (`file:line` / test / command) |
|---|---|
| AC-1 | `scripts/check_task_docs.py:34-38` / `python3 scripts/check_task_docs.py` → exit 0（大文字小文字fixtureでの回帰テストも実施しPASS） |
| AC-2 | `scripts/update_task_index.py:140-149`（next_task動的算出）／`docs/task-INDEX.md:6`「次タスク: Q1」 |
| AC-3 | `back_cargo/rust-toolchain.toml:1-4`、`back_cargo/Cargo.toml:5`（`rust-version = "1.96"`）。実測 `rustc --version` → `1.96.0`。※文言とDD-10の関係はF4参照 |
| AC-4 | `Makefile:6`（`env -u DATABASE_URL SQLX_OFFLINE=true cargo check ...`）／ローカル実行で成功確認済み |
| AC-5 | `docs/task-Q1-ci-quality-gate.md:56` `[ ]` — **未達**（CI実行証跡なし） |
| AC-6 | 同上 — **未達** |
| AC-7 | `.github/workflows/ci.yml:63-78`／`security-audit.yml`全体（`secrets.*`未使用、`permissions: contents: read`のみ、`pull_request`で`pull_request_target`ではない） |
| AC-8 | `.github/workflows/security-audit.yml:3-6`（`schedule`+`workflow_dispatch`）／`Makefile`にaudit混入なし |
| AC-9 | `ci.yml:39,42,47,57` / `security-audit.yml:23,26`。`git ls-remote`で4件全て実SHAと一致確認済み（`rust-cache`はannotated tag `v2.9.2^{}`のderef先が正しく使われている） |
| AC-10 | `CLAUDE.md`, `.claude/skills/{spec,impl,review,fix}/SKILL.md`, `AGENTS.md:17,294` の`NN`→`ID`一般化自体は確認。ただし`AGENTS.md`にAC-10範囲外の追加ありF1参照 |
| AC-11 | `docs/ai/PROJECT.md:17-34`（Q1〜Q3を含む表、次タスク=Q1） |
| AC-12 | `docs/ai/CI-POLICY.md:23-40`（先回り記述削除、「Q1で確定」明記、「2種類を混ぜない」は維持） |

### Findings

| ID | Severity | File:line | 失敗シナリオ / 指摘 | 必要な修正 | 根拠 |
|---|---|---|---|---|---|
| F1 | ~~HIGH~~ **解消** | `AGENTS.md:15,118`（旧`:69-192,349-437`） | ~~task doc §4はAGENTS.md変更を「AC-10（`NN`→`ID`一般化）」に限定しているが、実際は345行規模の新規ガバナンスセクションが追加されていた~~ → Codexが新規セクションを全削除し、AC-10スコープの2箇所（`docs/task-NN-*.md`→`docs/task-ID-*.md`、「Task 07以降」→「managed task」）のみに縮小。fix cycle 2のreviewerが`git diff`で実測確認 | 対応不要（解消済み） | fix cycle 2 reviewer検証 |
| F1a | ~~HIGH~~ **解消** | （該当セクション削除により消滅） | ~~`.env`無条件読み取り自動承認がCLAUDE.mdと矛盾~~ → 原因セクションごと削除されたため解消 | 対応不要（解消済み） | fix cycle 2 reviewer検証 |
| F1b | ~~LOW~~ **解消** | （該当セクション削除により消滅） | ~~`sed`の誤分類~~ → 原因セクションごと削除されたため解消 | 対応不要（解消済み） | fix cycle 2 reviewer検証 |
| F2 | HIGH | `docs/task-Q1-ci-quality-gate.md:16,56-57` | 完了条件は「CI上で`verify` jobが実際にgreenになる」ことを明示要求しているが、AC-5/AC-6は未達（Implementation Recordは「commit/pushしていないため未取得」と正直に記載）。`scripts/check_task_docs.py:141`は`status=done`時の未完了ACをFAILさせるため、現状のまま`done`化は不可能 | 人間が本差分をpush/PRし、`verify` jobのgreen run URL/run IDをImplementation Recordへ追記した上でAC-5/AC-6を`[x]`にする。Codexの実装欠陥ではなくtask doc自身がpush権限を持たないCodexに達成不能な完了条件を課している構造的な問題 | task doc `:16,:56-57`／`scripts/check_task_docs.py:141`／AGENTS.md:317（Codexはpush禁止） |
| F3 | LOW | `docs/implementation-plan.md`（全体） | 1000行規模の差分。§1で「Q1着手前から存在する未コミット差分」として土台化は授権済みだが、Task 07の`depends_on`変更や章番号の再構成など書式変換以外の実質変更もあり、task doc §4に記載がない | 次回以降、§4の想定変更箇所に実質変更を伴うファイルを明記する。内容はAC/DDと整合しており修正必須ではない | task doc §1:34-48, §4:92-97 |
| F4 | MEDIUM | `docs/task-Q1-ci-quality-gate.md:54` vs `:76` | AC-3の文言「baseイメージタグにバージョン番号を重複記述しない」と、DD-10「`rust-toolchain.toml`のchannelと一致する具体バージョンタグに固定する（例: `rust:1.96.0-slim-bookworm`）」が字面上矛盾する。実装はDD-10を採用（`back_cargo/Dockerfile:4`）しており判断自体は妥当 | ~~task doc内の記載矛盾をSpec Deviationsへ記録し、AC-3の文言をDD-10と整合する表現へ修正する~~ → **対応済み**: `/fix Q1`時にClaudeがAC-3文言をDD-10と整合する表現へ直接修正した（task doc記載のみの修正のためCodex対応不要） | task doc `:54,:76`／`back_cargo/Dockerfile:4` |
| F5 | LOW | `.github/workflows/ci.yml:12-14` | `cancel-in-progress: true`が`push: main`にも適用されるため、Q2でrequired checkにした際、main連続pushで進行中の`verify` runがcancelされたまま残る可能性がある | Q2の設計時に、main pushの`concurrency`設定見直しを検討事項として引き継ぐ | `.github/workflows/ci.yml:3-14` |
| F6 | LOW | `scripts/update_task_index.py`（末尾）／`back_cargo/rust-toolchain.toml`（末尾） | 末尾に改行が無い（同diffが他ファイルの同種問題を修正しているのに新規発生） | 各ファイル末尾に改行を追加する | — |

### Review disposition

- [ ] BLOCKED
- [x] CHANGES REQUESTED
- [ ] READY

**fix cycle 1**（Sonnet→Opus 2段階）: BLOCKER 0 / HIGH 3（F1, F1a, F2）/ MEDIUM 1（F4）/ LOW 3（F1b, F3, F5, F6）→ CHANGES REQUESTED。

**fix cycle 2**（F1/F1a/F1b/F4対応後、Sonnet再レビュー）: F1/F1a/F1b/F4は解消確認。残るのはBLOCKER 0 / HIGH 1（F2のみ）/ MEDIUM 0 / LOW 2（F5, F6、いずれも今回対応不要）→ **CHANGES REQUESTED**（唯一の理由はF2）。

F2はCodexの実装修正では解決できない（push権限がない）。人間が本差分をPR/pushし、GitHub Actions上で`verify` jobが実際にgreenになった実行証跡（run URL/run ID）をImplementation Recordへ追記した後、AC-5/AC-6を`[x]`にして再度`/review Q1`を実行すればREADYへ遷移できる見込み。AGENTS.mdのスコープ超過という残課題は解消済みで、それ以外の実装（CI/toolchain/scripts/AI scaffold/方針文書）に技術的な欠陥は残っていない。

## 12. つまずいた点と教訓

<実装時に発生した問題と、次回再発防止になる知識。>

## 13. 次タスクへの引き継ぎ

- Q2はrequired check候補として `verify` job名をそのまま使う（DD-1）。job名を変更しないこと。
- Q1のCIはNeon/production secretsを一切使わない設計。Q2のbranch protection設定時もこの前提を崩さない。
- `cargo audit`（`security-audit.yml`）はrequired checkにしない。Q2のRuleset設定でaudit workflowを必須化しない。

## 14. 再現コマンド

```bash
export DATABASE_URL="postgres://ops_hub:ops_hub@localhost:5433/ops_hub"
make verify
```
