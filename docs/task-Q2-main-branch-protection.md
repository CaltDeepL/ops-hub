---
id: "Q2"
slug: main-branch-protection
status: APPROVED
# Historical task migrated to the v3 status vocabulary.
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
- [x] AC-2: `scripts/github/main-ruleset.json`（望ましいRuleset設定の正本、POST body形式）が存在し、`python3 -c "import json;json.load(open(...))"` でparseでき、DD-4・DD-19の最小ルール集合を含む
- [ ] AC-3: `scripts/github/verify_main_ruleset.py` が無認証GETのみでlive設定を取得し、DD-20（訂正後）のexact比較対象（`rules[].type`集合・`required_status_checks[].context`集合）を含めて`main-ruleset.json`との比較に成功すればexit 0、不一致・未適用ならexit 1で差分をstderrへ出す。secretを一切要求・出力しない（fix cycle 2: exact-set比較は実装・動作確認済み。親Claudeが実live実行で`$.main.required_status_checks[].context: unexpected live values ['audit']`が単独検出されることを確認した。ただし現行コードは`bypass_actors`欠落を無条件でfatalな差分として扱っており、DD-20訂正によりこの比較を削除しない限り、正本と一致した場合でも恒久的にexit 1になる欠陥が残っている。未達）
- [x] AC-4: `scripts/check_ruleset_contract.py` が `make verify` に追加され、(a) `main-ruleset.json` の required status check contextが `ci.yml` の `jobs.*.name` に実在する、(b) `ci.yml` のtriggerに `paths`/`paths-ignore` が存在しない、をオフラインで検証してexit 0。job名を変えた一時fixtureでexit 1になることを確認する
- [ ] AC-5: 人間がRulesetを適用した後、`GET /repos/CaltDeepL/ops-hub/rulesets` が1件（`target: branch`, `enforcement: active`）を返すことをClaude Codeが無認証GETで確認する（適用前 `[]` との対比を記録）
- [ ] AC-6: 適用後、`GET /repos/CaltDeepL/ops-hub/rules/branches/main` が `deletion` / `non_fast_forward` / `pull_request` / `required_status_checks` の4 typeを返すことを確認する（適用前 `[]` との対比を記録）
- [ ] AC-7: classic branch protectionを併用していないことを確認する（人間が `gh api /repos/CaltDeepL/ops-hub/branches/main/protection` を実行し404 `Branch not protected` を確認、または同等の確認結果を記録）
- [ ] AC-8（failure path）: `verify` が確実に失敗する使い捨てPRで、(a) `GET /commits/<head_sha>/check-runs` の `verify` が `conclusion: failure`（Claudeが無認証実測）、(b) 人間が `gh api /repos/.../pulls/<n> --jq .mergeable_state` で `blocked` を確認、(c) GitHub UI上でmergeボタンが無効化されていることを記録する。このPRはmergeせずcloseする
- [ ] AC-9（success path）: (a) Q2自身の実装PRが `verify` green後にPR経由でmergeできる、(b) `git push origin main` の直接pushが `GH013`（ruleset違反）で拒否されることを、実際のエラー出力抜粋つきで記録する
- [ ] AC-10: 適用後の実際のRuleset JSON（`gh api /repos/.../rulesets/<id>` の出力）をImplementation Recordへ記録する。token等のsecretを含めない（fix cycle 2、DD-20訂正により追記: この認証済み出力には`bypass_actors`フィールドが含まれる。人間が記録し、Claude Codeがその内容を確認して`bypass_actors: []`であることをここで検証する。無認証の`verify_main_ruleset.py`では検証しない）
- [ ] AC-11: merge後の`main`最新commitに `verify` check runが存在し `conclusion: success` であることを確認する（post-merge検証signalが機能している証拠）
- [ ] AC-12: `docs/ai/PROJECT.md` の「現在地点」表がQ2の実際の状態（AC-5〜13完了後にdone）と次タスクQ3を反映し、`docs/ai/CI-POLICY.md` に短いQ2セクション（mainはPR経由のみ・required checkは`verify`のみ・`audit`は必須化しない・緊急時手順への参照）が追加される。変更はこの2ファイルの該当箇所に限定する（fix cycle 1: 現在はQ2 blocked・DD-20の設計確認および人間によるRuleset再適用と実証待ち。DD-21によりdoneへの更新はDD-18⑤で行う）
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
          "require_extra_approval_for_unattributed_changes": false,
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

### fix cycle 1（review後の精緻化）

- DD-19: `scripts/github/main-ruleset.json` の `pull_request.parameters` に `"require_extra_approval_for_unattributed_changes": false` を追加する。GitHub側の既定値は `true` であり、DD-5（単独開発者運用のためapproval 0固定）と同種のデッドロック（自分のPRに対する追加approval要求）を別経路で再導入する死角だったため（fix cycle 1 reviewer-critical検出）。`docs/adr/0001-main-branch-protection-ruleset.md` 内の引用JSONも同時に更新する
- DD-20: DD-12を精緻化する。`rules[].type`の集合・`required_status_checks[].context`の集合は**exact比較**とし、live側の余分な要素も差分として検出する。それ以外のフィールド（GitHubが自動補完するdefault key）はsubset許容のまま維持する（fix cycle 1で導入、fix cycle 2実装済み。`python3 scripts/github/verify_main_ruleset.py` の実live出力で `$.main.required_status_checks[].context: unexpected live values ['audit']` が単独で検出されることを確認済み）
- DD-20訂正（fix cycle 2）: 当初「`GET /rulesets/{id}`（detail endpoint）を追加取得すれば`bypass_actors`が読める」としたが誤りだった。Codexの実装（fix cycle 2）で無認証 `GET /rulesets/{id}` を実際に叩いたところ、レスポンスに`bypass_actors`キー自体が存在しないことが判明し（Spec Deviationとして正しく報告・停止）、親Claudeが `curl -s https://api.github.com/repos/CaltDeepL/ops-hub/rulesets/22847969` で独立に再現確認した（GitHub公式仕様: `bypass_actors`はrulesetへのwrite accessを持つrequesterにのみ返される）。したがって**`bypass_actors`の検証は`verify_main_ruleset.py`（無認証専用、DD-11）のスコープから除外する**。欠落を空配列へ正規化することは「非公開＝空」という未検証の前提を混入させるため行わない。`bypass_actors`が空であることの確認はAC-10（人間が認証済み`gh api .../rulesets/<id>`の出力を記録し、Claude Codeがその記録内容を確認する）に委ねる
- DD-21: AC-12（`docs/ai/PROJECT.md` / `docs/ai/CI-POLICY.md` のQ2状態反映）はDD-18の手順⑤（Ruleset実測・failure/success path実演が完了した後の最終task doc更新）に紐づけて実行する。①の実装コミット時点では、`PROJECT.md` の「現在地点」表にQ2を確定済み（done）として書かない。実装コミット時点の適切な表現は「Q2: review中（AC-5〜13は人間によるRuleset適用・実証待ち）」等、実情に即したものにする

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

実装中に DD-* と矛盾する判断が必要になった場合、コードを書く前に「何を・なぜ」をここへ記録し、`status: BLOCKED` にして停止する。Human/Claude architectが設計を更新するまで再開しない。

- 現在のBLOCKED理由: GitHub Rulesetの再適用、認証済み画面でのbypass確認、failure/success pathの実演はHuman操作が必要。ローカル実装だけではAC-5〜AC-11とAC-13を完了できない。
- ~~DD-20は無認証の`GET /repos/{owner}/{repo}/rulesets/{id}`から`bypass_actors`を取得して配列全体をexact比較するよう要求するが、実際の無認証GETではRuleset id `22847969`のdetail responseに`bypass_actors`キーが存在しなかった~~ → **解決済み（fix cycle 2、親Claude）**: `curl -s https://api.github.com/repos/CaltDeepL/ops-hub/rulesets/22847969` で独立に再現確認し、DD-20を訂正した（`bypass_actors`の検証を`verify_main_ruleset.py`のスコープから除外し、AC-10の認証済み人間確認に委ねる）。詳細は §3 DD-20訂正を参照。Codexへの残作業は`verify_main_ruleset.py`から`bypass_actors`比較コードを削除すること（次のfix cycle）。

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

### GitHub Ruleset

- 無認証GETでRuleset `main-protection`（id `22847969`、target `branch`、enforcement `active`）の存在までは確認済み。
- live設定は正本と不一致で、`audit`混入、`strict_required_status_checks_policy: true`、`required_review_thread_resolution: true`、`require_extra_approval_for_unattributed_changes: true`が既知の問題。適合確認は人間による再適用後に行う。

### Verification PR

- PR: #4
- `verify`: PASS
- Rulesetが正本と不一致のためmergeabilityは未確認。failure/success path、direct push拒否、force push拒否、branch削除拒否は人間による確認待ち。


_Codexが実装完了時に更新する。_

### 変更ファイル（fix cycle 1）

- `scripts/github/main-ruleset.json`
- `scripts/github/verify_main_ruleset.py`
- `docs/ai/PROJECT.md`
- `docs/task-Q2-main-branch-protection.md`

### 実装上の判断

- DD-19に従い追加approval既定値を明示的に`false`とした。
- DD-20に従い、mainへ適用されるrule type・required contextの集合をexact比較へ変更した。`bypass_actors`のexact比較は、無認証detail responseでpropertyを取得できないためSpec Deviationとして停止した。
- DD-21に従い、PROJECTのQ2状態を実際のtask statusと人間による再適用待ちへ戻した。

### DB / APIへの影響

- 変更なし。migration / query / `.sqlx/` / HTTP APIは変更していない。

### Verification evidence

```text
make verify
未実行。DD-20とDD-11のSpec Deviationを検出したため、statusをblockedにして停止した。
```

狭い検証では、Ruleset JSON parseとcontract checkはexit 0。一致fixtureはexit 0、bypass actor追加fixtureとrequired context追加fixtureはそれぞれexit 1となった。実APIへの無認証GETでは、Ruleset detailに`bypass_actors`が含まれないことを確認した。

### 残課題

- Claude architectによるDD-20の更新待ち。あわせて、人間がRuleset id `22847969`を正本JSONで再適用する必要がある。AC-5〜AC-11・AC-13と、DD-18⑤でのAC-12完了は、その後の人間による実演およびClaude Codeの確認・記録待ち。

## 11. Review Record

_Claude Code親セッションがread-only reviewerの結果を転記する。_

### Verification

- [ ] `make verify` の実際の成功結果を確認した。（reviewer-criticalはDocker未接続のためcargo/npm部分を再現できず、`check_ruleset_contract.py`とライブAPI検証のみ独立確認した。Implementation Recordの`make verify` exit 0主張は未検証のまま）
- [ ] 全Acceptance Criteriaを具体的diff/test証拠へ対応付けた。（AC-1/AC-4は再現確認済み。AC-2/AC-3/AC-12は今回のfix cycle 1指摘により未達へ変更。AC-5〜11/13は証拠なし）

### Acceptance evidence

| Criterion | Evidence (`file:line` / test / command) |
|---|---|
| AC-1 | `git show 11bd018 -- .github/workflows/ci.yml`（concurrencyブロック2行のみ変更、`on:`/`jobs.verify.name`/各step無変更）— 達成 |
| AC-2 | `scripts/github/main-ruleset.json` — DD-19により`require_extra_approval_for_unattributed_changes: false`の追記が必要。現行ファイル未追記のため未達 |
| AC-3 | fix cycle 2: exact-set比較（`rules[].type`, `required_status_checks[].context`）は実装済み。親Claudeが`python3 scripts/github/verify_main_ruleset.py`を実行し、`$.main.required_status_checks[].context: unexpected live values ['audit']`が単独で正しく検出されることを確認（F4主要部分は解消）。ただし`bypass_actors`比較が残っており、DD-20訂正で削除するまで一致時でも恒久的にexit 1。未達 |
| AC-4 | `python3 scripts/check_ruleset_contract.py` → exit 0。job名を`renamed`に変えた一時fixture → exit 1（reviewer-critical再現確認済み）。`Makefile:6` に1行追加。達成 |
| AC-5 | `GET /repos/CaltDeepL/ops-hub/rulesets` → 1件（`id:22847969`, `target:branch`, `enforcement:active`）実測。ただし内容がDD-4と不一致（AC-6参照）のため「適用済みだが正本と不一致」として未達扱い |
| AC-6 | `GET /repos/CaltDeepL/ops-hub/rules/branches/main` → `deletion`/`non_fast_forward`/`pull_request`/`required_status_checks`の4 typeは存在するが、`required_status_checks`に`audit`混入、`strict_required_status_checks_policy:true`、`required_review_thread_resolution:true`、`require_extra_approval_for_unattributed_changes:true`の4点がDD-4/DD-7/DD-14と不一致（F1参照）。未達 |
| AC-7 | `GET /branches/main/protection` → `401`（無認証のため未確認。認証済み確認が必要）。未達 |
| AC-8 | 使い捨て失敗PRの実演が未実施。未達 |
| AC-9 | PR #4は`mergeable_state: unstable`（`audit`要求が満たせないため）。直接push拒否の実演も未記録。未達 |
| AC-10 | Implementation Recordは箇条書きの要約であり、`gh api .../rulesets/22847969`の実JSON出力ではない。未達 |
| AC-11 | mainへの本PRのmergeが未完了（`merged: false`）。未達 |
| AC-12 | `docs/ai/PROJECT.md`がQ2を"done"と記載（`git show 11bd018 -- docs/ai/PROJECT.md`）だが、AC-5〜11/13未達・DD-21のタイミング規定に反するため未達 |
| AC-13 | `docs/ai/CI-POLICY.md`の新設Q2セクションと task doc §13に緊急時手順の文言は存在するが、§2のcheckboxは未反映。内容確認後に対応 |

### Findings

| ID | Severity | File:line | 失敗シナリオ / 指摘 | 必要な修正 | 根拠 |
|---|---|---|---|---|---|
| F1 | BLOCKER | live `GET /rules/branches/main`（ファイルなし）／`scripts/github/main-ruleset.json`（DD-4正本）／`.github/workflows/security-audit.yml:3-6` | 実際に適用されたRuleset（`id:22847969`）が正本と4点で不一致: (a) `required_status_checks`に`audit`混入（`security-audit.yml`は`schedule`/`workflow_dispatch`のみでPRイベントで絶対に起動しないため、この要求は原理的に満たせない）, (b) `strict_required_status_checks_policy:true`（DD-7は`false`）, (c) `required_review_thread_resolution:true`（DD-4は`false`）, (d) `require_extra_approval_for_unattributed_changes:true`（DD-5のapproval 0デッドロック回避と衝突する別経路）。PR #4（Q2自身の実装PR）は`mergeable_state:unstable`。ただし「恒久的にmerge不可」は`workflow_dispatch`手動実行で部分的に回避可能なため過大表現（reviewer-critical訂正）で、実際のmerge阻止は認証済み確認（`gh pr view --json mergeStateStatus`）が必要 | 人間がRuleset（id `22847969`）を `gh api --method PUT /repos/CaltDeepL/ops-hub/rulesets/22847969 --input scripts/github/main-ruleset.json`（DD-19適用後の正本）で再適用し、`audit`削除・`strict:false`・`required_review_thread_resolution:false`・`require_extra_approval_for_unattributed_changes:false`を反映する。Codex/Claudeは実行しない（DD-10） | fix cycle 1 reviewer + reviewer-critical、live API実測 |
| F2 | ~~HIGH~~ **解消** | `docs/task-Q2-main-branch-protection.md` §10 | ~~Implementation Recordが`security-audit: PASS`という虚偽を含み、PR番号が未記入だった~~ → Codexが撤回・書き直し。`git diff -- docs/task-Q2-main-branch-protection.md`のfix cycle 2差分を親Claudeが確認: PR番号は実際の`#4`、`security-audit: PASS`の記載なし、未確認項目は「人間による確認待ち」と正直に記載 | 対応不要（解消済み） | fix cycle 2、親Claude直接確認 |
| F3 | ~~MEDIUM~~ **解消** | `docs/ai/PROJECT.md` | ~~Q2を"done"と記載~~ → `git diff -- docs/ai/PROJECT.md`を親Claudeが確認: 「Q2 \| main branch protection \| blocked（DD-20の設計確認待ち）」に修正され、本文もDD-21に沿って実情（人間によるRuleset再適用待ち）を記載 | 対応不要（解消済み） | fix cycle 2、親Claude直接確認 |
| F4 | ~~HIGH~~ **主要部分解消、残作業あり** | `scripts/github/verify_main_ruleset.py` | ~~list-subset比較が要素追加を検出できない~~ → `rule_types`/`required_status_contexts`のexact-set比較関数が追加され、親Claudeが実live実行で`$.main.required_status_checks[].context: unexpected live values ['audit']`が単独検出されることを確認。**残作業**: `bypass_actors`のexact比較コード（`exact_array_differences`呼び出しと、`subset_differences`に渡す`desired`辞書内の`bypass_actors`キー）が、DD-20訂正により削除対象と判明。現状のままでは正本と一致してもこのチェックが恒久的にexit 1を返す | `verify_main_ruleset.py`から`bypass_actors`比較コードを削除する（DD-20訂正参照、次のfix cycle） | fix cycle 2、親Claude直接確認・実行 |
| F5 | HIGH | `git show 11bd018 --stat`（`docs/task-Q1-ci-quality-gate.md` 749+/190-） | Q2の実装コミット`11bd018`に、Q2 §4の想定変更箇所に無い`docs/task-Q1-ci-quality-gate.md`（739行規模）が混在している。DD-16（変更範囲をtask doc §4のファイルに限定）違反。内容自体は改竄されていない（DD/AC/READY判定は保持）ことをreviewer-criticalが確認済み | 該当commitは既にorigin/infrastructure/ci-setupへpush済み・PR #4に紐づくため、履歴を書き換える指示が無い限り本レビューでは追加の訂正アクションを取らない。今後のcommit分離を運用上の注意点として引き継ぐ（本findingへの追加コード修正は不要） | fix cycle 1 reviewer-critical |
| N3 | MEDIUM | live ruleset `created_at:2026-09-11T00:31:57Z` / `updated_at:2026-09-11T00:48:09Z` | 作成後に編集された痕跡があり、DD-18②の`gh api --method POST ... --input scripts/github/main-ruleset.json`という手順ではなくGitHub UIでの個別設定に見える（F1の4点不一致の発生源と整合） | 再適用時は必ず正本JSONを`--input`で渡す（UIのチェックボックス操作をしない） | fix cycle 1 reviewer-critical |
| N4 | LOW | live `required_status_checks[].context`の`audit`エントリ | `verify`は`integration_id:15368`でpinされているが`audit`エントリには`integration_id`が無く、任意のappや同名commit statusで満たせてしまう（`audit`自体はF1で削除予定のため実害は限定的） | F1の修正（`audit`削除）で解消。念のためDD-4正本に将来同種のcontextを追加する際は`integration_id`指定を必須にする運用注意として記録 | fix cycle 1 reviewer-critical |
| F6 | ~~LOW~~ **解消** | `docs/task-Q2-main-branch-protection.md` | ~~`## 10. Implementation Record`の節番号が欠落~~ → 復旧済み（本doc §10見出しで確認） | 対応不要（解消済み） | fix cycle 2、親Claude直接確認 |
| F7 | LOW | `scripts/github/verify_main_ruleset.py`（`fail()`内のメッセージ生成） | 親Claudeの実live実行で`$.ruleset.bypass_actors: missing from live settings`（subset_differencesから）と`$.ruleset.bypass_actors: exact array comparison requires arrays; desired list, live NoneType`（exact_array_differencesから）が同一原因について重複出力される | F4の`bypass_actors`比較コード削除で自然に解消する見込み。独立した修正は不要 | fix cycle 2、親Claude直接確認 |

### Review disposition

- [ ] BLOCKED
- [x] CHANGES REQUESTED
- [ ] READY

**fix cycle 1**（Sonnet→Opus 2段階）: BLOCKER 1（F1、内訳4点）/ HIGH 3（F2, F4, F5）/ MEDIUM 2（F3, N3）/ LOW 2（N4, F6）→ **CHANGES REQUESTED**。

設計自体（DD-1〜DD-18, ADR 0001）はreviewer-criticalにより妥当と判定され、`status: blocked`による`/spec Q2`への差し戻しは不要と結論。ただしDD-4正本JSON・DD-12の比較方式・AC-12の実行タイミングに限定的な精緻化（DD-19〜DD-21）が必要と判明したため、本レビューでtask doc・ADRへ反映した。

次のアクション:
1. **人間**: Ruleset（id `22847969`）をDD-19適用後の正本JSONで`PUT`により再適用し、AC-7〜11・AC-13の実演・記録を行う（Codex/Claudeは実行しない、DD-10）。AC-10の記録時に認証済みJSON中の`bypass_actors`が空配列であることも確認する
2. **Codex（`/fix Q2`）**: `scripts/github/verify_main_ruleset.py`から`bypass_actors`比較コードを削除する（F4残作業、DD-20訂正参照）
3. F5は既にpush済みのcommitに混在した問題であり、履歴書き換えの指示が無い限り追加アクションは取らない

### fix cycle 2（F2/F3/F4/F6対応、DD-19/DD-20対応）

Codexが`/fix Q2`でF2・F3/DD-21・F4（exact-set比較部分）・F6・DD-19（JSON1フィールド追加）に対応した。作業中に、DD-20が指示した「`GET /rulesets/{id}`から`bypass_actors`を取得する」という前提が無認証APIでは成立しないことを発見し、正しく`status: blocked`＋Spec Deviationsで自己停止した（AGENTS.mdのSpec Deviationsルールどおりの挙動）。

親Claudeが以下を直接検証した（今回はSonnet reviewerを介さず、diffが小さく機械的に再現可能だったため親Claude自身が`file:line`と実行結果で検証。理由: F2/F3/F6はテキスト差分の目視確認、F4は`python3 scripts/github/verify_main_ruleset.py`の実行結果確認、Spec Deviationは`curl`によるGitHub API再現確認で、いずれも高確度に独立検証可能だったため）。

- `git diff -- scripts/github/main-ruleset.json` → DD-19のフィールド追加のみ。確認
- `git diff -- docs/task-Q2-main-branch-protection.md` の Implementation Record → F2解消（PR #4記載、虚偽記載なし）、F6解消（見出し番号復旧）
- `git diff -- docs/ai/PROJECT.md` → F3/DD-21解消（実情に即した記述）
- `python3 scripts/github/verify_main_ruleset.py` 実行 → exact-set比較が`audit`混入を単独検出することを確認（F4主要部分解消）。ただし`bypass_actors`比較が残り恒久的にexit 1（F4残作業）
- `curl -s https://api.github.com/repos/CaltDeepL/ops-hub/rulesets/22847969` → `bypass_actors`キーが存在しないことを確認。Codexの Spec Deviation報告が正確であることを検証
- 親ClaudeがDD-20を訂正（`bypass_actors`比較をスコープ外にし、AC-10の認証済み確認に委ねる）し、AC-10の記述にも追記した

結果: BLOCKER 1（F1、変化なし。人間の対応待ち）/ HIGH 1（F4残作業のみ。F2・F3は解消） / MEDIUM 1（N3、変化なし） / LOW 2（N4, F7）→ **CHANGES REQUESTED**（継続）。設計は今回の訂正で再び安定しており、`status: blocked`は不要と判断し`fixing`へ戻す。

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
