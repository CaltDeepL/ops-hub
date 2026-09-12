# Task 21: Dependabot を導入

## 1. 前提確認

着手前にGitHubの現状を確認したところ、task-17〜20は既にPR#7〜#9経由でmainにマージ済みでした。加えて、そちらの側でMakefile/ci.ymlをさらに手直しされていました。

- `Makefile修正`: `verify`ターゲットを`assert_local_database_url.py` / `check_ruleset_contract.py` / AI tooling tests / npm lint・build呼び出し無しの、`cd back_cargo && cargo fmt/clippy/sqlx prepare --check/test`だけを行う最小構成に簡略化
- `fix:ci.yml修正`: `cargo sqlx migrate run`ステップを追加

2点目は私の設計漏れの指摘でした。`cargo sqlx prepare --check`はDATABASE_URLが指す実DBのスキーマと突き合わせるため、事前にmigrationを当てておく必要があります（`#[sqlx::test]`はテストごとに別DBを作って自動適用するため、こちらとは別に必要)。この修正は適切です。

以上を踏まえ、今回はこの最新main(`0f762ce`)を起点にしています。

## 2. 実施したこと

`.github/dependabot.yml`を新規作成し、3エコシステムを対象にしました。

| ecosystem | directory | 対象 |
|---|---|---|
| npm | `/` | フロントエンド(package.json) |
| cargo | `/back_cargo` | Rustバックエンド(Cargo.toml) |
| github-actions | `/` | `.github/workflows/*.yml` |

github-actionsを含めたのは、ci.yml / security-audit.ymlの`uses:`がfull commit SHA固定(`# vX.Y.Z`コメント付き)になっているため。Dependabotはこの形式を認識し、SHAとコメントを両方更新するPRを作ります。

`open-pull-requests-limit: 5`、`commit-message.prefix`は`.githooks/commit-msg`のConventional Commitsパターンに合わせて npm/cargo を`chore`、github-actions を`ci`としました。グルーピングや`schedule.day`指定は入れず、最小構成での導入としています。

## 3. 検証したこと

```bash
python3 -c "import yaml; yaml.safe_load(open('.github/dependabot.yml'))"
# YAML OK
```

GitHub側の実際のDependabot動作(スケジュール通りにPRが作られるか)はマージ後の実地確認が必要です。

## 4. 次タスクへの引き継ぎ

- `main-protection` Rulesetのlive設定差分(`strict_required_status_checks_policy` / `required_review_thread_resolution` / `require_extra_approval_for_unattributed_changes`)が`scripts/github/main-ruleset.json`の期待値と一致していない点が残課題
- 現在の簡略化されたMakefileには`scripts/assert_local_database_url.py`・`scripts/check_ruleset_contract.py`・`scripts/run_quiet.py`が呼ばれずに残っている(削除するか、使う形に戻すかは未決定)
