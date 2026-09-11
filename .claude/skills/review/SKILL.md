---
name: review
description: Codex実装後のops-hub taskを証拠ベースでread-onlyレビューし、親ClaudeだけがReview Recordと状態を更新する。
argument-hint: "ID"
disable-model-invocation: true
---

Task `$0` をレビューする。

1. task docのAcceptance Criteria / Required Tests / Invariants / Implementation Recordを読み、次に`git diff --stat`と実際のdiffを読む。
2. managed taskが`IMPLEMENTED`であることを確認する。review開始時にstatusを変更しない。
3. diffを分類する。
   - `migrations/` / `.sqlx/`
   - lock / transaction / retry / idempotency / run coordination
   - incident state transition
   - HTTP API
4. migration、lock/concurrency、incident不変条件の変更を含む場合は `reviewer-critical` を使う。それ以外は `reviewer` を使う。
5. reviewerにRequired Testsの欠落確認、AC→具体的証拠対応、Invariants違反確認、findingの`file:line`と根拠を必須にする。
6. 通常 reviewer がBLOCKERを出した場合は `reviewer-critical` で再レビューする。
7. Agentはファイルを編集しない。親Claudeが task doc のみ更新する。
   - Review Record
   - frontmatter status
8. READY条件:
   - BLOCKER/HIGHが0
   - Codex ACがすべて具体的証拠へ対応し`[x]`
   - Human — Immediate ACがHumanにより完了済み
   - 実際に成功した `make verify` 証拠を確認
   - 未解決Spec Deviationsなし
   - Required Testsの欠落なし
   - Human ACをAIが完了扱いにしていない
9. READYならClaudeだけが`status: READY`へ変更する。DONEにはしない。
10. 修正可能なfindingがあれば`FIX`とし、statusは`IMPLEMENTED`のまま`/fix $0`へ戻す。
11. 設計そのものの矛盾なら`BLOCKED` + `status: BLOCKED`としてHuman/architectへ戻す。
12. `make task-index` を実行する。
13. task docだけで判断できない場合のみ関連test/呼び出し元へ広げ、さらに必要な場合だけ該当ADR・上位設計の該当箇所を読む。
14. implementation code、test、migration、`.sqlx/` は編集しない。
