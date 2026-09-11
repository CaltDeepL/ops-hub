# ops-hub — Workflow compatibility note

共通workflowと安全境界の正本は`AGENTS.md`、Claude固有のroutingは`CLAUDE.md`、task状態の一覧は`docs/task-INDEX.md`とする。この文書は過去taskからのlink互換性のためだけに残す。

```text
/spec → DRAFT
Human Gate → APPROVED
/impl → IMPLEMENTED
/review → READY / FIX / BLOCKED
Human commit/push → DONE
```

Codexは承認済みtask、任意の`MANIFEST.md`、Filesの対象、関連testから読み始める。不足時だけ`AGENTS.md`のL1/L2へ広げ、L3では停止する。最終品質ゲートは`make verify`、依存脆弱性検査は別の`make audit`とする。
