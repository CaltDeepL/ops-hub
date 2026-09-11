# Task 19: AI協働フレームワーク(v3 task doc)の残骸を削除

採番は19。task-08(→18)の続きとして着手予定だった「ci.ymlの実質検証化」に着手する前に、これが必要と判明したため間に挟んだ。

## 1. 背景

task-18(back_cargo復元)の後、`docs/task-INDEX.md`を`scripts/update_task_index.py`で生成させたところ、このリポジトリには本来16タスク+Q1〜Q3という正式なロードマップがあり、`scripts/check_task_docs.py`が`docs/task-(Q[1-9]\d*|\d{2})-.*\.md`という命名のファイルに対してfrontmatter(`id`/`status`)とv3構造・分類済みACを機械的に要求することが判明した。

さらに履歴を辿ると、`AGENTS.md` / `CLAUDE.md` / `docs/ai/{PROJECT,WORKFLOW,CI-POLICY}.md` / `docs/templates/task-template-v3.md` / `.claude/skills/{spec,impl,review,fix}/SKILL.md` という、人間・Claude Code・Codex・architectの役割分担を定義したAI協働フレームワーク一式が存在していたが、`a772d7a`(task6まで巻き戻した)と`4e6802f`(マージ事故)の2箇所で失われ、現treeには存在しないことを確認した。

## 2. 対応方針(ユーザー指示)

このAI協働フレームワークは復元せず、完全に削除する方針とした。今回の実用的な3修正(旧task-07〜09、新task-17〜20)はそのまま活かし、ロードマップ本来の番号(Q1〜Q3、07〜16)と衝突しないよう番号を繰り上げる。

## 3. 実施したこと

- `scripts/check_task_docs.py`・`scripts/update_task_index.py`とその対応テスト(`scripts/tests/test_check_task_docs.py`、`scripts/tests/test_update_task_index.py`)を削除。これらはv3フレームワーク専用のvalidator/index生成器で、他の用途では使われていない
- `Makefile`の`verify`ターゲットから`@python3 scripts/check_task_docs.py`行を削除、`task-index`ターゲット自体を削除(`.PHONY`からも除去)
- `docs/task-INDEX.md`は`update_task_index.py`の生成物のみでgit管理下になかったため、削除対象コミットには含まれない
- `AGENTS.md`等のフレームワーク本体ファイルは現treeに既に存在しないため、削除操作自体は不要(該当なし)

## 4. 検証したこと

```bash
python3 -m unittest discover -s scripts/tests -p 'test_*.py'
# Ran 2 tests in 0.147s / OK (残ったのはtest_run_quiet.pyのみ)

python3 scripts/check_ruleset_contract.py
# ruleset contract check passed: contexts=['verify'], no paths/paths-ignore triggers
```

`scripts/assert_local_database_url.py` / `scripts/run_quiet.py` / `scripts/check_ruleset_contract.py` / `scripts/github/*` はv3フレームワークと無関係(CI/DB安全性/Ruleset契約チェック用の汎用ツール)なので削除していない。

## 5. 次タスクへの引き継ぎ

- task-20でci.ymlに`make verify`を組み込む(この時点でMakefileはcheck_task_docs.py呼び出しが既に除去済みの版を前提にする)
- 今後のtask doc命名は、機械的なvalidatorが無くなったため強制力はないが、人間の可読性のため`docs/task-NN-*.md`の連番は維持し、正式ロードマップ(01〜16、Q1〜Q3)の番号とは重複させない方針で17から採番を続ける
- 正式ロードマップの07(stale-lock-recovery)以降の機能タスクに実際に着手する場合、番号の再整理(17〜20を別の接頭辞に移すなど)を検討してもよいが、現時点では未着手
