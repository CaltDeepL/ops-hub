# Task 19: AI 協働フレームワーク（v3 task doc）の残骸を削除

| 項目 | 内容 |
|---|---|
| 目的 | 現 tree に本体が存在しない AI 協働フレームワーク専用の validator / index 生成器を削除し、品質ゲートから外す |
| 完了条件 | 残骸スクリプトと Makefile の呼び出しが無くなり、残った Python tooling tests と Ruleset 契約チェックが PASS する |
| 採番 | 19。「ci.yml の実質検証化」（Task 20）の前に必要と判明したため間に挟んだ |
| ステータス | 完了 |

## 1. 実施内容

### 背景

Task 18（back_cargo 復元）の後、`docs/task-INDEX.md` を `scripts/update_task_index.py` で生成させたところ、このリポジトリには本来16タスク + Q1〜Q3 という正式なロードマップがあり、`scripts/check_task_docs.py` が `docs/task-(Q[1-9]\d*|\d{2})-.*\.md` という命名のファイルに対して frontmatter（`id` / `status`）と v3 構造・分類済み AC を機械的に要求することが判明した。

さらに履歴を辿ると、`AGENTS.md` / `CLAUDE.md` / `docs/ai/{PROJECT,WORKFLOW,CI-POLICY}.md` / `docs/templates/task-template-v3.md` / `.claude/skills/{spec,impl,review,fix}/SKILL.md` という、人間・Claude Code・Codex・architect の役割分担を定義した AI 協働フレームワーク一式が存在していたが、`a772d7a`（task6まで巻き戻した）と `4e6802f`（マージ事故）の2箇所で失われ、現 tree には存在しないことを確認した。

### 削除したもの

- `scripts/check_task_docs.py`・`scripts/update_task_index.py` とその対応テスト（`scripts/tests/test_check_task_docs.py`、`scripts/tests/test_update_task_index.py`）。v3 フレームワーク専用の validator / index 生成器で、他の用途では使われていない
- `Makefile` の `verify` ターゲットから `@python3 scripts/check_task_docs.py` 行を削除、`task-index` ターゲット自体を削除（`.PHONY` からも除去）
- `docs/task-INDEX.md` は `update_task_index.py` の生成物のみで git 管理下になかったため、コミット対象外
- `AGENTS.md` 等のフレームワーク本体ファイルは現 tree に既に存在しないため、削除操作は不要

## 2. 設計判断

- **フレームワークは復元せず、完全に削除する（ユーザー指示）。** 本体が失われた状態で validator だけ残しても、存在しない規約を機械的に強制するだけになる
- **汎用ツールは残す。** `scripts/assert_local_database_url.py` / `scripts/run_quiet.py` / `scripts/check_ruleset_contract.py` / `scripts/github/*` は v3 フレームワークと無関係（CI / DB 安全性 / Ruleset 契約チェック用）なので削除しない
- **採番は正式ロードマップと衝突させない。** 今回の実用的な修正（旧 task-07〜09 → 新 task-17〜20）はそのまま活かし、ロードマップ本来の番号（Q1〜Q3、07〜16）と衝突しないよう17から採番を続ける

## 3. つまずいた点と教訓

- index 生成を試して初めて、失われたフレームワークの規約が validator 経由で生き残っていたことが判明した。**本体が消えた仕組みの「残骸」は、動かしてみるまで気づきにくい。** 履歴（`git log --all`）で本体の有無を確認してから、残す・消すを判断する

## 4. 再現コマンド

```bash
python3 -m unittest discover -s scripts/tests -p 'test_*.py'
# Ran 2 tests / OK（残ったのは test_run_quiet.py のみ）

python3 scripts/check_ruleset_contract.py
# ruleset contract check passed: contexts=['verify'], no paths/paths-ignore triggers
```

## 5. 次タスクへの引き継ぎ

- Task 20 で ci.yml に `make verify` を組み込む（Makefile は `check_task_docs.py` 呼び出しが除去済みの版を前提にする）
- 今後の task doc 命名は、機械的な validator が無くなったため強制力はないが、人間の可読性のため連番は維持し、正式ロードマップ（01〜16、Q1〜Q3）の番号とは重複させない
- 正式ロードマップの07（stale-lock-recovery）以降の機能タスクに着手する場合、番号の再整理（17〜20を別の接頭辞に移すなど）を検討してもよい（現時点では未着手）
