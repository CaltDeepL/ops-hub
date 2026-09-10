# ops-hub — Claude Code + Codex 開発ワークフロー

## 目的

会話履歴ではなく、repository内のtask docをClaude CodeとCodexのhandoff契約にする。

## Task 07以降の状態

```text
planned
  ↓ /spec
spec
  ↓ /impl
implementing
  ├─ 設計矛盾 → blocked → /spec
  └─ 実装完了 → review
                     ├─ READY → done
                     ├─ 実装修正 → fixing → review
                     └─ 設計問題 → blocked → /spec
```

許可値:

- `planned`
- `spec`
- `implementing`
- `blocked`
- `review`
- `fixing`
- `done`

Task 01〜06はlegacy task docsをそのまま保存し、`docs/task-INDEX.md` ではdoneとして扱う。

## 1. Spec — Claude Code

```text
/spec 07
```

Claudeは既存16タスクロードマップ、直前task handoff、実装コードを読み、必要ならread-only architectを使う。

出力:

- Goal / Non-goals
- AC-*
- DD-*
- expected implementation area
- DB/API compatibility
- ADR
- validation

この段階でproduction codeは変更しない。

## 2. Implementation — Codex

```text
/impl 07
```

Claudeはtaskを `implementing` に変更し、Codex用実装プロンプトだけを作る。

Codexはtask docとAGENTS.mdを契約として実装する。

設計矛盾を発見したら:

- `Spec Deviations` へ記録
- `status: blocked`
- 矛盾する実装を止める
- `/spec NN` に戻す

## 3. Verification — Codex

反復中の狭いtestは任意。

最終ゲート:

```bash
make verify
```

`make verify` はコード品質を検証するため、`implementing` / `fixing` 中の未チェックACを理由に失敗させない。

task文書の厳格な完了条件は `status: done` のときだけ機械検査する。

## 4. Review — Claude Code

```text
/review 07
```

通常diffはSonnet reviewer。

次はOpus reviewer-critical:

- migration / `.sqlx/`
- advisory lock / transaction / concurrency
- idempotency / duplicate execution
- incident不変条件
- SonnetがBLOCKERを検出

reviewerはread-only。
親ClaudeだけがReview Record/statusを更新する。

READY条件:

1. BLOCKER/HIGH = 0
2. 全ACが具体的証拠へ対応
3. 全AC `[x]`
4. 実際の成功した `make verify` 証拠を確認
5. 未解決Spec Deviations = 0

## 5. Fix — Codex

```text
/fix 07
```

実装上のfindingだけをCodexへ渡す。
設計を変えないと直せないfindingは `/spec` へ戻す。

## 6. Commit — 人間

Claude/Codexは通常フローでcommitしない。

`status: done` + Review READY + verify証拠確認後、人間がcommitする。

既存運用どおり `docs/commits/task-NN-<slug>.txt` にコミット文面を残す。

## 7. task index

状態変更後:

```bash
make task-index
```

`docs/task-INDEX.md` は生成物として扱う。

## 8. 並列作業

Claude CodeとCodexが同じworking treeを同時編集しない。
独立taskを並列化する場合のみ別worktree/branchを使う。
docs/ai/checklists/concurrency.md