---
name: review
description: Codex実装後のops-hub taskを証拠ベースでread-onlyレビューし、親ClaudeだけがReview Recordと状態を更新する。
argument-hint: "ID"
disable-model-invocation: true
---

Task `$0` をレビューする。

1. task doc、`docs/ai/WORKFLOW.md`、実際の `git diff` を読む。
2. managed taskは status を `review` にし、`make task-index` を実行する。
3. diffを分類する。
   - `migrations/` / `.sqlx/`
   - lock / transaction / retry / idempotency / run coordination
   - incident state transition
   - HTTP API
4. migration、lock/concurrency、incident不変条件の変更を含む場合は `reviewer-critical` を使う。それ以外は `reviewer` を使う。
5. reviewerにAC→具体的証拠対応、findingの `file:line` と根拠を必須にする。
6. 通常 reviewer がBLOCKERを出した場合は `reviewer-critical` で再レビューする。
7. Agentはファイルを編集しない。親Claudeが task doc のみ更新する。
   - Acceptance Criteria checkbox
   - Review Record
   - frontmatter status
8. READY条件:
   - BLOCKER/HIGHが0
   - 全ACが実証され `[x]`
   - 実際に成功した `make verify` 証拠を確認
   - 未解決Spec Deviationsなし
9. READYなら `status: done`。
10. BLOCKER/HIGHがあれば `CHANGES REQUESTED` + `status: fixing`。
11. 設計そのものの矛盾なら `BLOCKED` + `status: blocked` として `/spec $0` へ戻す。
12. `make task-index` を実行する。
13. implementation code、test、migration、`.sqlx/` は編集しない。
