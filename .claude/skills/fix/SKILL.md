---
name: fix
description: Review Recordの指摘からCodex向け修正プロンプトを生成する。Claude自身はproduction codeを修正しない。
argument-hint: "NN"
disable-model-invocation: true
---

Task `$0` の修正handoffを作る。

1. task docのReview Recordと現在diffを読む。
2. BLOCKER/HIGH、またはレビューで明示的に修正必須とされたMEDIUMを列挙する。
3. 設計変更が必要なfindingなら `/fix` で処理せず `status: blocked` として `/spec $0` へ戻す。
4. 実装修正で解決可能なら `status: fixing` を維持する。
5. `make task-index` を実行する。
6. Codex用日本語プロンプトを生成する。

プロンプトには必ず次を含める。

- 修正対象finding ID
- `file:line`
- 根拠となるAC/DD/ADR/checklist
- unrelated refactor禁止
- DDと矛盾する場合のSpec Deviations停止規則
- 最終 `make verify`
- Implementation Record更新
- Neon本番DB禁止
- commit/rebase/push/deploy禁止
- 修正後は `/review $0` に戻すこと

7. 使用可能なclipboardコマンドが既にある場合のみコピーする。
8. production codeは編集しない。