# ops-hub — Claude Code 運用ルール

## 役割分担

Claude Code は **仕様化・設計・レビュー・障害解析** を担当する。
Codex は **実装・レビュー指摘修正** を担当する。

標準フロー:

```text
/spec ID
  ↓
/impl ID
  ↓ Codex
/review ID
  ├─ READY → 人間がcommit
  └─ CHANGES REQUESTED → /fix ID → Codex → /review ID
```

`ID` にはQ系列（例: `Q1`）またはTask 07以降の数値系列（例: `07`）を指定する。

Claude Code と Codex に同じworking treeを同時編集させない。

## 正本

- 16タスク全体: `docs/implementation-plan.md`
- AI向け現在状態: `docs/ai/PROJECT.md`
- handoff規約: `docs/ai/WORKFLOW.md`
- managed taskの作業契約: `docs/task-ID-*.md`
- 長期的な設計理由: `docs/adr/`

Task 01〜06 の既存文書へ機械管理用frontmatterを後付けしない。

## Agent routing

### architect — Opus

次を含む設計で使う。

- migration / schema
- advisory lock / transaction / concurrency
- incident状態遷移
- idempotency / outbox
- API互換性
- CI / deployment / failure domain

read-only agent。実装ファイルを書かせない。

### reviewer — Sonnet

通常の実装diffレビュー。
read-only agent。

### reviewer-critical — Opus

次の場合に使用する。

- `migrations/` を含む
- lock / concurrency / transaction / idempotency の意味が変わる
- incident状態遷移の不変条件が変わる
- Sonnet reviewer が BLOCKER を出した

### debugger — Sonnet

compile/test/SQLx/PostgreSQL/Docker/runtime/CI の原因調査。
read-only agent。

## チェックリストの分岐

reviewer本体に全ルールを詰め込まない。

diffに応じて読む。

- migration / `.sqlx/` → `docs/ai/checklists/migration.md`
- lock / retry / transaction / run coordination → `docs/ai/checklists/concurrency.md`
- incident / liveness / dead-man → `docs/ai/checklists/incident.md`
- HTTP API → `docs/ai/checklists/api.md`

## 最終検証

最終品質ゲートは1つだけ。

```bash
make verify
```

個別コマンドを「同等」と解釈して代用しない。

## DB / secrets

- Neon本番DBへ接続しない。
- `.env` / `.env.*` の秘密情報を読まない。
- 本番URL、token、Slack Webhookをログやtask docへ貼らない。
- 権限制約を回避するコマンドを組み立てない。

## Git

通常フローでcommit/push/rebaseを行わない。

review dispositionがREADY、task statusがdone、`make verify`証拠が揃った後に人間がコミットする。
