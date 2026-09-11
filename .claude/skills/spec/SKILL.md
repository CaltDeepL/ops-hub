---
name: spec
description: 指定したops-hub managed taskを日本語で仕様化し、task docを作成・更新する。実装は禁止。
argument-hint: "ID [補足要件]"
disable-model-invocation: true
---

ops-hub Task `$0` の仕様化フェーズを実行する。

1. `AGENTS.md`、`CLAUDE.md`、対象task docを読む。ロードマップ・ADR・上位設計は必要箇所だけを検索して読む。
2. 直前タスクの「次タスクへの引き継ぎ」、関連コード/test/migration、必要なADR・上位設計の該当箇所をこのフェーズで調査する。
3. schema/API/concurrency/reliabilityの実質的判断がある場合は `architect` を使う。
4. managed taskは `docs/templates/task-template-v3.md` を基準に `docs/task-$0-<slug>.md` を作成/更新する。
5. frontmatterを `status: DRAFT` にする。Claude自身で`APPROVED`にしない。
6. Goal / Scope / Out of Scope / Invariants / Files / Required Tests / Design Decisions / Likely Pitfalls / Reproduction / Verify / Handoffを具体化する。FilesのModifyには実在path、Addには新規pathを列挙する。
7. ACは`Codex` / `Human — Immediate` / `Human — Deferred`に分け、`AC-1`形式で通し番号を付ける。DeferredにはDueを付ける。
8. InvariantsとDesign Decisionsを分離し、関連ADR・上位設計の結論をCodexが再調査しなくてよい粒度へ圧縮する。
9. Codexが実装時に新しいarchitectureを発明しなくてよい粒度まで決める。
10. 長期的な設計理由は必要に応じてADRへ分離する。
11. `docs/commits/task-$0.txt`へ推奨commit messageを作り、一時`MANIFEST.md`へ変更予定pathと目的を記す。
12. `implementation/`は新規・自己完結・弱結合・既存参照2〜3箇所以内・signature確定済みの場合だけ使用する。それ以外のproduction code、migration、testは変更しない。
13. `python3 scripts/check_task_docs.py`と`make task-index`を実行する。
14. Human Gateの確認対象、task path、決定事項、未解決事項を短く返す。

Task 01〜06の既存task文書にはfrontmatterを後付けしない。

追加コンテキスト: `$ARGUMENTS`
