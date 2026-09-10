---
name: spec
description: 指定したops-hubタスクを日本語で仕様化し、Task 07以降のtask docを作成・更新する。実装は禁止。
argument-hint: "NN [補足要件]"
disable-model-invocation: true
---

ops-hub Task `$0` の仕様化フェーズを実行する。

1. `CLAUDE.md`、`docs/implementation-plan.md`、`docs/ai/PROJECT.md`、`docs/ai/WORKFLOW.md` を読む。
2. 対象タスクの既存文書、直前タスクの「次タスクへの引き継ぎ」、関連コード/test/migrationを読む。
3. schema/API/concurrency/reliabilityの実質的判断がある場合は `architect` を使う。
4. Task 07以降は `docs/templates/task-template-v3.md` を基準に `docs/task-$0-<slug>.md` を作成/更新する。
5. frontmatterを `status: spec` にする。
6. ACは `AC-1` 形式、設計不変条件は `DD-1` 形式で番号を付ける。
7. Codexが実装時に新しいarchitectureを発明しなくてよい粒度まで決める。
8. 長期的な設計理由は必要に応じてADRへ分離する。
9. production code、migration、testは変更しない。
10. `make task-index` を実行する。
11. task path、決定事項、未解決事項を返す。

Task 01〜06の既存task文書にはfrontmatterを後付けしない。

追加コンテキスト: `$ARGUMENTS`