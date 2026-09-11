---
name: fix
description: Review Recordの指摘からCodex向け修正プロンプトを生成する。Claude自身はproduction codeを修正しない。
argument-hint: "ID"
disable-model-invocation: true
---

Task `$0` の修正handoffを作る。

1. task docのReview Record、Acceptance Criteria、Invariants、現在diffを読む。全ADR・上位設計は再読しない。
2. BLOCKER/HIGH、またはレビューで明示的に修正必須とされたMEDIUMを列挙する。
3. 設計変更が必要なfindingなら`/fix`で処理せず`status: BLOCKED`としてHuman/architectへ戻す。
4. 実装修正で解決可能なら`status: IMPLEMENTED`を維持する。
5. `make task-index` を実行する。
6. Codex用日本語プロンプトを生成する。

プロンプトには必ず次を含める。

- 修正対象finding ID
- `file:line`
- 根拠となるAC/Invariants/DD/ADR
- unrelated refactor禁止
- Invariantsと矛盾する場合のL2→Spec Deviations→L3停止規則
- Required Testsの再確認
- 最終 `make verify`
- Codex ACだけの更新と短いImplementation Record更新
- Neon本番DB禁止
- commit/rebase/push/reset/clean/tag/release/merge/deploy禁止
- 修正後は `/review $0` に戻すこと

7. 使用可能なclipboardコマンドが既にある場合のみコピーする。
8. production codeは編集しない。
