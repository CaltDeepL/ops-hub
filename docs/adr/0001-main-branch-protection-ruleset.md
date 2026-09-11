# ADR 0001：main branch protectionをGitHub Repository Rulesetで実装する

- Status: Accepted
- Date: 2026-09-11
- Related tasks: Q2

## Context

`docs/implementation-plan.md` §12「Q2 — main branch protection」は、Q1で構築した`verify` job（品質ゲート）をmainのmerge条件として強制することをゴールに定めているが、機構は「GitHub Ruleset / branch protection」と両論併記のまま残している。どちらを採用するかは、Claude Codeによるread-only独立検証の可否・単独開発者運用での実行可能性・緊急時の復旧手段という3つの長期的制約に直結し、後から書き換えると設定の整合性を壊しやすいため、ADRとして決定を固定する。

architectによる実測（2026-09-11、無認証GitHub REST API）:

- `GET /repos/CaltDeepL/ops-hub/rulesets` → `200 []`（無認証でも読める）
- `GET /repos/CaltDeepL/ops-hub/branches/main/protection` → `401 Requires authentication`
- `GET /repos/CaltDeepL/ops-hub/branches/main` → `protected: false`（現時点でどちらも未設定）
- `git log` のauthorはほぼ1人（単独開発者運用）

## Decision

**Repository Ruleset（`/repos/{owner}/{repo}/rulesets`）を採用し、classic branch protection（`/branches/{branch}/protection`）は使わない。**

main保護の正本はRuleset 1件のみとし、次の最小ルール集合に固定する。

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

不変条件:

1. `required_approving_review_count` は `0` に固定する。単独開発者はGitHub上で自分のPRを自分でapproveできないため、`1`以上にするとmainへの更新が完全に不可能になる。
2. `bypass_actors` は空のまま維持する。緊急時（Actions障害・CI恒久失敗）は常設bypassを作らず、`enforcement` を `active` → `disabled`（または `evaluate`）へ一時変更し、復旧後に戻す運用とする。この操作はGitHub側の監査ログに残る。
3. `strict_required_status_checks_policy` は `false` に固定し、これと`.github/workflows/ci.yml`のpush(main) concurrencyをcommit単位に分離する変更（Q2 DD-8）を**セットの判断**として維持する。mainがrebase必須（strict）にならない代わりに、push(main)ごとの`verify` runをcancelさせず、post-merge検証signal（semantic merge conflictの検出）を保全する。
4. required status checkのcontextは `verify` のみとする。`cargo audit`（`security-audit.yml`）はrequired checkにしない（Q1からの継続）。
5. Ruleset本体の作成・変更・削除はCodex・Claude Codeのいずれも実行しない。admin権限のtokenを取得・要求・保存しない。実際の適用は人間が行い、Claude Codeは無認証GETによる独立検証のみを担う。
6.（fix cycle 1で追記）`pull_request.parameters.require_extra_approval_for_unattributed_changes` は `false` に固定する。GitHub側の既定値は `true` であり、有効化されるとPR内に作者へ帰属できないcommitがある場合に追加approvalを要求する。これは決定4の「approval 0」不変条件と同じ単独開発者制約から生じる別経路のデッドロックであり、レビューで検出された（Q2 fix cycle 1、reviewer-critical）。

## Consequences

**利点:**

- `rulesets` / `rules/branches/{branch}` エンドポイントは無認証でも読み取れるため、Claude Codeがread-onlyで独立に設定を検証できる。Q1のAC-5/AC-6（人間がCI実行 → Claudeが証跡をAPIで確認）と同じ分業パターンをQ2でも維持できる。classic branch protectionは無認証で読めず（401実測）、この検証ループが成立しない。
- Rulesetは1件のみで運用するため、classicとの二重管理・「最も厳しい方が勝つ」という合成規則の不透明化を避けられる。
- `enforcement`のdry-runモード（`evaluate`）とrule insightsにより、緊急時の一時解除が監査ログに残る形で行える。

**コスト・運用影響:**

- GitHub Actionsの可用性がmainへのmergeのクリティカルパスに入る。Actions障害・runner枯渇時はmainへ何もmergeできなくなる（緩和策: `enforcement`の一時変更、Q2 AC-13）。
- 単独開発者運用を前提に`required_approving_review_count: 0`を採用しているため、協働者が増えた場合はこの判断のsupersedeが必要になる（下記Alternatives参照）。
- `strict=false`を採用したことで、2つの個別にgreenなPRのmerge後にmainが壊れる（semantic merge conflict）を検出する手段は、push(main)の`verify` run（DD-8のconcurrency分離により保全）のみになる。

**互換性・migration影響:**

- HTTP API（`/v1/runs`等）・DB schema・migrationには一切影響しない。リポジトリ運用ルールのみの変更。

## Alternatives considered

### Classic branch protection（`/branches/main/protection`）

無認証で読めない（401実測）ため、Claude Codeによる独立検証ループが成立しない。Rulesetと併用すると「両方の設定のうち最も厳しい方が有効になる」という合成規則が働き、どちらが実際の保護状態を決めているかが不透明になる。却下。

### `required_approving_review_count` を1以上にする

GitHubは自分のPRを自分でapproveできない。単独開発者運用でこれを1以上にすると、mainへのmerge手段が完全に失われる（運用負荷の増加ではなく、デッドロック）。却下。協働者が増えた場合は本ADRをsupersedeして値を見直す。

### Merge queue（`merge_group` trigger）

`ci.yml`へのtrigger追加とCI実行増を伴うが、単独開発者運用では得られる利得がない。cancelled runの扱いがより致命的になる（queue全体が止まる）。却下。

### `strict_required_status_checks_policy: true`（up-to-date必須）

main更新のたびに未mergeの全PRがrebase＋CI再実行を要求される。Q3で導入予定のDependabot PR群がこれを増幅する。post-merge の`verify` run（DD-8）で同等の検出目的を代替できるため、コストに対して利得がない。却下。

### `bypass_actors`にrepo adminを常設

緊急時のmerge手段としては機能するが、ゲートが実質的にadvisory（いつでも回避可能）になり、Q1で確立した「`make verify`が単一の品質ゲート」という原則と矛盾する。`enforcement`の一時変更（監査ログに残る）で緊急時対応は十分に足りるため却下。

### `required_signatures`

ローカルのcommitは現状未署名（GitHub側のmerge commitのみ`verified: true`）。有効化すると現行の開発フローが即座に破綻する。却下（将来必要になれば別ADRで検討）。

### `required_linear_history`

現行の運用はmerge commit方式（PR #1〜#3すべてmerge commit）。今切り替える利得がない。却下。
