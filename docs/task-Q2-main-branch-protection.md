---
id: "Q2"
slug: main-branch-protection
status: implementing
# planned -> spec -> implementing -> review -> fixing -> review -> done
# 設計矛盾時: blocked -> spec
depends_on: ["Q1"]
---

# タスクQ2：main branch protection

| 項目 | 内容 |
|---|---|
| 上位ドキュメント | `docs/implementation-plan.md` §12「Q2 — main branch protection」 |
| ゴール | Q1で構築した品質ゲート（`verify` job）を `main` branchのmerge条件として強制する |
| 完了条件 | `main` がGitHub Repository Rulesetで保護され、required status check `verify` が機能し、CI失敗時に保護された変更をmergeできないことを実際に実証し、設定内容をtask docへ記録する |
| 実装範囲外 | `back_cargo/src/**` / `migrations/**` / `tests/**`（機能実装）、Dependabot（Q3）、GitHub Actions monitor（Task 15）、classic branch protection、merge queue |

## 1. Context / 現状

Q1（CI / Quality Gate基盤）は`status: done`・Review disposition READYで完了済み。以下がQ2の前提として確定している。

- `.github/workflows/ci.yml` の唯一のjobは `name: verify`（job idも`verify`）。これがrequired status check候補（Q1 DD-1）。job名は変更しない。
- CIはPostgreSQL 17 service containerのみを使い、Neon/production secretsを一切参照しない（Q1 DD-3、維持する）。
- `cargo audit`（`.github/workflows/security-audit.yml`）はrequired checkにしない（Q1引き継ぎ事項）。
- Q1 Review Record Findings F5（LOW、Q1では対応不要としてQ2へ引き継ぎ）: `.github/workflows/ci.yml` の `concurrency.cancel-in-progress: true` が `push: main` にも適用されており、mainへのrequired check運用時にpost-merge検証signalが失われるリスクがある。

architectによるGitHub側の実態確認（無認証REST API、2026-09-11時点）:

- `GET /repos/CaltDeepL/ops-hub` → `private: false`, `default_branch: main`
- `GET /repos/CaltDeepL/ops-hub/rulesets` → `[]`（Ruleset 0件）
- `GET /repos/CaltDeepL/ops-hub/rules/branches/main` → `[]`（mainに適用中のrule 0件）
- `GET /repos/CaltDeepL/ops-hub/branches/main` → `protected: false`（classic branch protectionも未設定）
- `GET /repos/CaltDeepL/ops-hub/commits/<sha>/check-runs` → `verify` job（app id 15368 = github-actions）がcheck run名として実測される
- PR経由の運用（PR #1〜#3、すべてmerge済み）はすでに定着している
- `render.yaml`: `autoDeploy: false`（CI green後にDeploy Hookで手動/別トリガー起動）→ Q2はdeploy経路に影響しない

つまり、mainを保護する設定は現時点で一切存在せず、衝突なく新規に設計できる状態にある。

## 2. Acceptance Criteria

- [x] AC-1: `.github/workflows/ci.yml` の `concurrency` ブロックのみを変更し、`pull_request` では同一PRの古いrunをcancelしたまま、`push`(main) はcommit（SHA）単位のgroupに分離してcancelされないようにする。`on:` / `jobs.verify.name`（`verify`）/ 各stepは無変更（`git diff` で確認）
- [x] AC-2: `scripts/github/main-ruleset.json`（望ましいRuleset設定の正本、POST body形式）が存在し、`python3 -c "import json;json.load(open(...))"` でparseでき、DD-4の最小ルール集合を含む
- [x] AC-3: `scripts/github/verify_main_ruleset.py` が無認証GETのみでlive設定（`rulesets` / `rules/branches/main`）を取得し、`main-ruleset.json` との subset比較に成功すればexit 0、不一致・未適用ならexit 1で差分をstderrへ出す。secretを一切要求・出力しない
- [x] AC-4: `scripts/check_ruleset_contract.py` が `make verify` に追加され、(a) `main-ruleset.json` の required status check contextが `ci.yml` の `jobs.*.name` に実在する、(b) `ci.yml` のtriggerに `paths`/`paths-ignore` が存在しない、をオフラインで検証してexit 0。job名を変えた一時fixtureでexit 1になることを確認する
- [ ] AC-5: 人間がRulesetを適用した後、`GET /repos/CaltDeepL/ops-hub/rulesets` が1件（`target: branch`, `enforcement: active`）を返すことをClaude Codeが無認証GETで確認する（適用前 `[]` との対比を記録）
- [ ] AC-6: 適用後、`GET /repos/CaltDeepL/ops-hub/rules/branches/main` が `deletion` / `non_fast_forward` / `pull_request` / `required_status_checks` の4 typeを返すことを確認する（適用前 `[]` との対比を記録）
- [ ] AC-7: classic branch protectionを併用していないことを確認する（人間が `gh api /repos/CaltDeepL/ops-hub/branches/main/protection` を実行し404 `Branch not protected` を確認、または同等の確認結果を記録）
- [ ] AC-8（failure path）: `verify` が確実に失敗する使い捨てPRで、(a) `GET /commits/<head_sha>/check-runs` の `verify` が `conclusion: failure`（Claudeが無認証実測）、(b) 人間が `gh api /repos/.../pulls/<n> --jq .mergeable_state` で `blocked` を確認、(c) GitHub UI上でmergeボタンが無効化されていることを記録する。このPRはmergeせずcloseする
- [ ] AC-9（success path）: (a) Q2自身の実装PRが `verify` green後にPR経由でmergeできる、(b) `git push origin main` の直接pushが `GH013`（ruleset違反）で拒否されることを、実際のエラー出力抜粋つきで記録する
- [ ] AC-10: 適用後の実際のRuleset JSON（`gh api /repos/.../rulesets/<id>` の出力）をImplementation Recordへ記録する。token等のsecretを含めない
- [ ] AC-11: merge後の`main`最新commitに `verify` check runが存在し `conclusion: success` であることを確認する（post-merge検証signalが機能している証拠）
- [x] AC-12: `docs/ai/PROJECT.md` の「現在地点」表がQ2 doneと次タスクQ3を反映し、`docs/ai/CI-POLICY.md` に短いQ2セクション（mainはPR経由のみ・required checkは`verify`のみ・`audit`は必須化しない・緊急時手順への参照）が追加される。変更はこの2ファイルの該当箇所に限定する
- [ ] AC-13: 緊急時手順（Actions障害・CI恒久失敗時に `enforcement` を一時的に `disabled` へ落として復旧後 `active` に戻す）がtask docに記録され、Task 16 Runbookへの引き継ぎ事項として明記される

## 3. 設計判断・不変条件

### Ruleset方式

- DD-1: required status checkのcontextは `verify` のみに固定する。`jobs.verify.name` を変更しない、matrix化しない、job分割しない
- DD-2: `.github/workflows/ci.yml` のtriggerに `paths` / `paths-ignore` を追加しない（docs-only PRで `verify` が起動せず、required checkが永久pendingになるため）
- DD-3: `ci.yml` に `workflow_dispatch` / `merge_group` を追加しない（前者はPR文脈外でrequired checkを満たす経路を作る。後者はmerge queue前提でQ2のscope外）
- DD-4: Rulesetの最小ルール集合は次の4種類に固定する。ルールの追加・削除はSpec Deviation扱いとする。

  ```json
  {
    "name": "main-protection",
    "target": "branch",
    "enforcement": "active",
    "conditions": { "ref_name": { "include": ["~DEFAULT_BRANCH"], "exclude": [] } },
    "bypass_actors": [],
    "rules": [
      { "type": "deletion" },
      { "type": "non_fast_forward" },
      {
        "type": "pull_request",
        "parameters": {
          "required_approving_review_count": 0,
          "dismiss_stale_reviews_on_push": false,
          "require_code_owner_review": false,
          "require_last_push_approval": false,
          "required_review_thread_resolution": false,
          "allowed_merge_methods": ["merge", "squash", "rebase"]
        }
      },
      {
        "type": "required_status_checks",
        "parameters": {
          "strict_required_status_checks_policy": false,
          "do_not_enforce_on_create": false,
          "required_status_checks": [ { "context": "verify", "integration_id": 15368 } ]
        }
      }
    ]
  }
  ```

- DD-5: `required_approving_review_count` は `0` に固定する。単独開発者運用ではGitHubが自分のPRを自分でapproveできないため、`1`以上にするとmainへ何もmergeできなくなる（運用負荷ではなく完全なデッドロック）
- DD-6: `bypass_actors` は空のままにする。緊急時は常設bypass actorではなく、`enforcement` の一時変更（AC-13）で対応する
- DD-7: `strict_required_status_checks_policy: false` と、push(main)のconcurrency SHA単位分離（DD-8）は**セットの判断**であり、片方だけを変更しない
- DD-8: push(main)のconcurrency groupはcommit（`github.sha`）単位に分離し、`cancel-in-progress` は `pull_request` イベントのみ `true` にする。`cancel-in-progress: false` へ単純変更するだけでは不十分（GitHubは同一concurrency groupでpending runを1件しか保持せず、groupを分離しないと連続mergeで中間commitのrunが結局cancelledになる）
- DD-9: 「mainのcancelled runが後続PRのrequired checkをブロックする」という説明は誤りとして扱う（required checkはPR head SHAの最新check runで評価されるため、mainのcancelled runはPRの評価に影響しない）。DD-8の修正理由はpost-merge検証signal（AC-11）の保全であり、この理由付けを変えない

### 適用主体・権限境界

- DD-10: Ruleset作成・変更・削除（`POST`/`PUT`/`DELETE /repos/.../rulesets`）はCodex・Claude Codeのいずれも実行しない。admin権限のtokenを取得・要求・保存・task docへ記載しない。実際の適用は人間が行う
- DD-11: `scripts/github/verify_main_ruleset.py` は無認証GETのみを使う。認証を要求・実装しない。`make verify` には含めない（GitHub APIの可用性・rate limitを最終品質ゲートに持ち込まない。Q1 DD-2と同じ思想）
- DD-12: `verify_main_ruleset.py` の比較は「desired JSON ⊆ live」のsubset比較とする。GitHubがread-backで既定keyを補うため、完全一致比較にしない
- DD-13: classic branch protection（`/branches/main/protection`）を設定しない。main保護の正本はRuleset 1件のみとする

### Q1からの引き継ぎ遵守

- DD-14: `cargo audit` / `security-audit.yml` をrequired checkにしない。`.github/workflows/security-audit.yml` を変更しない
- DD-15: Q1で確定した事項（`verify` job名、GitHub ActionsのフルSHA pin方針、CIがNeon/production secretsを使わない、`ci.yml` と `monitor.yml` の責務分離、`make verify` 単一ゲート）を変更しない

### スコープ・実施順序

- DD-16: 変更範囲は本task doc §4のファイルに限定する。`back_cargo/**` / `migrations/**` / `tests/**` / `AGENTS.md` / `CLAUDE.md` / `docs/implementation-plan.md` / `.github/workflows/security-audit.yml` / `render.yaml` を変更しない（Q1のF1・F3再発防止）
- DD-17: 新規Pythonスクリプト（`verify_main_ruleset.py`, `check_ruleset_contract.py`）は標準ライブラリのみを使う。PyYAML等の依存追加やCLI追加ライブラリを禁止する
- DD-18: 実施順序を次に固定する。②を①より前にしない（Q2の実装PR自体がAC-9のpositive path実証になる）。

  ```text
  ① Codexが実装PRを作成（concurrency変更 + scripts/github/* + check_ruleset_contract.py + Makefile + 限定的docs更新）
    ↓
  ② 人間がRulesetを適用（gh api --method POST .../rulesets --input scripts/github/main-ruleset.json）
    ↓
  ③ 人間 + Claudeがfailure path（AC-8）・success path（AC-9）を実演
    ↓
  ④ Claudeが無認証GETで設定を検証し、Implementation Recordへ記録
    ↓
  ⑤ 最終task doc更新（保護下のmainへPR経由でmerge）
  ```

## 4. 想定変更箇所

- `.github/workflows/ci.yml` — `concurrency` ブロックのみ（DD-8）
- `scripts/github/main-ruleset.json` — 新規。望ましいRuleset状態の正本（DD-4）
- `scripts/github/verify_main_ruleset.py` — 新規。無認証read-only検証（DD-11, DD-12）
- `scripts/check_ruleset_contract.py` — 新規。オフライン契約チェック（AC-4）
- `Makefile` — `verify` ターゲットへ `check_ruleset_contract.py` 呼び出しを1行追加
- `docs/ai/CI-POLICY.md` — Q2セクション追加（限定）
- `docs/ai/PROJECT.md` — 「現在地点」表とその直後の説明文のみ（Q1のDD-14と同じ限定方針）
- `docs/task-Q2-main-branch-protection.md` — 本task doc
- `docs/adr/0001-main-branch-protection-ruleset.md` — Claudeが本spec時に作成済み。Codexは変更しない
- `docs/task-INDEX.md` — `make task-index` の生成物。手動編集しない

対象外（変更しない）: `back_cargo/**`, `migrations/**`, `tests/**`, `.sqlx/**`, `AGENTS.md`, `CLAUDE.md`, `docs/implementation-plan.md`, `.github/workflows/security-audit.yml`, `render.yaml`。

ファイル名の微調整は許容するが、DD-* を変える必要が出た場合は Spec Deviations を使う。

## 5. DB / migration / SQLx

変更なし。Q2は `migrations/` / `.sqlx/` を一切変更しない。

## 6. API / 互換性

変更なし。Q2はHTTP API（`/v1/runs` 等）の契約・挙動に一切影響しない。main保護はリポジトリ運用ルールのみの変更。

## 7. ADR

`docs/adr/0001-main-branch-protection-ruleset.md`

Q1は既存の`implementation-plan.md`に設計理由があるためADR不要だったが、Q2は`implementation-plan.md` §12が「GitHub Ruleset / branch protection」と機構を決めずに残しており、選択理由（Ruleset採用、approval 0、strict=false、bypass無し）を記録する正本がどこにも存在しない。かつ「協働者が増えたらapproval 0→1に変える」という将来のsupersedeが確実に発生する判断のため、ADRとして分離する。

## 8. Spec Deviations

実装中に DD-* と矛盾する判断が必要になった場合、コードを書く前に「何を・なぜ」をここへ記録し、`status: blocked` にして停止する。Claude architectが設計を更新するまで再開しない。

- なし。

## 9. 検証

反復用の狭い検証:

```bash
python3 -c "import json;json.load(open('scripts/github/main-ruleset.json'))"
git diff -- .github/workflows/ci.yml
python3 scripts/check_ruleset_contract.py
python3 scripts/github/verify_main_ruleset.py
```

`verify_main_ruleset.py` は適用前は「Ruleset未作成」でexit 1になることを確認する（negative path）。

AC-5〜AC-11はローカルだけで確認できない。人間がRulesetを実際に適用し、failure/success pathを実演した上で、run/PR/ruleset IDや実際のAPIレスポンスをImplementation Recordへ記録する。

最終品質ゲート:

```bash
make verify
```

## 10. Implementation Record

_Codexが実装完了時に更新する。_

### 変更ファイル

- `.github/workflows/ci.yml`
- `scripts/github/main-ruleset.json`
- `scripts/github/verify_main_ruleset.py`
- `scripts/check_ruleset_contract.py`
- `Makefile`
- `docs/ai/CI-POLICY.md`
- `docs/ai/PROJECT.md`
- `docs/task-Q2-main-branch-protection.md`

### 実装上の判断

- CIのconcurrency groupは、`pull_request`では従来どおりPRのref単位、`push`では`github.sha`単位とした。`cancel-in-progress`は`pull_request`の場合だけ有効にし、main pushごとのpost-merge検証signalを保全した。
- RulesetのPOST body正本はDD-4の4ルールをそのままJSON化し、追加ルール・bypass actor・approval要件を加えていない。
- live検証は認証headerを持たないGETに限定した。Ruleset一覧とmainへの適用ruleを取得し、一致するRulesetのread-back詳細もGETして、GitHubが補う追加keyを許容するsubset比較を行う。
- オフライン契約チェックは標準ライブラリのJSON処理と正規表現だけを使い、required contextとjob表示名、および`on:`配下のpath filter不在を検証する。

### DB / APIへの影響

- 変更なし。migration / query / `.sqlx/` / HTTP APIは変更していない。

### Verification evidence

```text
env DATABASE_URL=postgres://ops_hub:ops_hub@localhost:5433/ops_hub make verify
exit 0

- ruleset contract check: passed（context `verify`、paths / paths-ignoreなし）
- cargo check / fmt / clippy / sqlx prepare --check: passed
- cargo test: 31 passed, 0 failed
- npm lint / build: passed
- check_task_docs.py: passed
```

狭い検証では、Ruleset JSON parseと通常のcontract checkがexit 0、job名を`renamed`へ変えた一時fixtureがexit 1となることを確認した。`verify_main_ruleset.py` は追加keyを含む一致read-back fixtureでexit 0、実リポジトリへの無認証GETではRuleset未作成を検出してexit 1となった。

### 残課題

- AC-5〜AC-11およびAC-13は、人間によるRuleset適用とfailure/success path実演、およびClaude Codeによるread-only確認・記録待ち。

## 11. Review Record

_Claude Code親セッションがread-only reviewerの結果を転記する。_

### Verification

- [ ] `make verify` の実際の成功結果を確認した。
- [ ] 全Acceptance Criteriaを具体的diff/test証拠へ対応付けた。

### Acceptance evidence

| Criterion | Evidence (`file:line` / test / command) |
|---|---|
| AC-1 | |
| AC-2 | |
| AC-3 | |
| AC-4 | |
| AC-5 | |
| AC-6 | |
| AC-7 | |
| AC-8 | |
| AC-9 | |
| AC-10 | |
| AC-11 | |
| AC-12 | |
| AC-13 | |

### Findings

| ID | Severity | File:line | 失敗シナリオ / 指摘 | 必要な修正 | 根拠 |
|---|---|---|---|---|---|
| — | — | — | — | — | — |

### Review disposition

- [ ] BLOCKED
- [ ] CHANGES REQUESTED
- [ ] READY

## 12. つまずいた点と教訓

<実装時に発生した問題と、次回再発防止になる知識。>

## 13. 次タスクへの引き継ぎ

- Q3のDependabot PRも`verify`を通す設計（trigger `pull_request`、secrets不要、approval 0）であることをQ2設計時に確認済み。Q3のRuleset整合確認で前提として使ってよい。
- Q3以前に`cargo audit`をrequired checkにしない（Q1・Q2で維持した前提を継続する）。
- 緊急時（Actions障害・CI恒久失敗）は`main-ruleset.json`の`enforcement`を`disabled`に一時変更し、復旧後`active`へ戻す（AC-13）。この手順はTask 16のRunbookへ引き継ぐ。

## 14. 再現コマンド

```bash
python3 scripts/check_ruleset_contract.py
python3 scripts/github/verify_main_ruleset.py
make verify
```
