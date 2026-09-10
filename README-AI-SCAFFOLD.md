# ops-hub — Claude Code + Codex 併用 scaffold v3（日本語）

この scaffold は、既存の ops-hub に **上書き導入するのではなく、AI運用レイヤーだけを追加する** ための確定版です。

## v3 の前提

現行 ops-hub の正本・履歴は維持します。

- `docs/implementation-plan.md`：16タスクのロードマップ正本
- `docs/task-01-*.md` 〜 `docs/task-06-*.md`：既存タスク記録。frontmatter を後付けしない
- `docs/commits/task-01-*.txt` 〜：既存コミット文面
- `justfile`：既存DB操作用。置換しない
- `.github/workflows/monitor.yml`：存在する場合は運用監視用。品質CIと混ぜない

AI用の機械可読状態管理は **Task 07以降** に適用します。

## 現在地点

- Task 01〜06：完了扱い
- 次タスク：Task 07「取り残し lock 回収」
- Task 07 では Task 06 の引き継ぎを守る
  - `pg_locks` の `classid` / `objid` から bigint advisory-lock key を復元
  - `pg_terminate_backend` 後の lock 再取得は1回だけ
  - 回収成功時のみ `recovered: true`
  - 回収不能・権限不足は `already_running` へフォールバック
  - `POST /v1/runs` の rate limit も Task 07 で入口仕様として閉じる

## 追加するもの

```text
ops-hub/
├── AGENTS.md
├── CLAUDE.md
├── Makefile
├── .codex/
│   └── config.toml
├── .claude/
│   ├── settings.json
│   ├── hooks/
│   │   └── format-rust.sh
│   ├── agents/
│   │   ├── architect.md
│   │   ├── reviewer.md
│   │   ├── reviewer-critical.md
│   │   └── debugger.md
│   └── skills/
│       ├── spec/SKILL.md
│       ├── impl/SKILL.md
│       ├── review/SKILL.md
│       └── fix/SKILL.md
├── .githooks/
│   └── commit-msg
├── docs/
│   ├── task-INDEX.md
│   ├── ai/
│   │   ├── PROJECT.md
│   │   ├── WORKFLOW.md
│   │   ├── CI-POLICY.md
│   │   └── checklists/
│   │       ├── migration.md
│   │       ├── concurrency.md
│   │       ├── incident.md
│   │       └── api.md
│   ├── adr/
│   │   └── README.md
│   └── templates/
│       ├── task-template-v3.md
│       ├── adr-template.md
│       └── commit-template.txt
└── scripts/
    ├── assert_local_database_url.py
    ├── check_task_docs.py
    └── update_task_index.py
```

## 最終検証

人間、Claude Code、Codex、将来の品質CIで最終品質ゲートを統一します。

```bash
export DATABASE_URL="postgres://ops_hub:ops_hub@localhost:5433/ops_hub"
make verify
```

個別の `cargo test --test runs_test` 等は実装中の反復用です。最終判定の代わりにはしません。

## 通常フロー

```text
/spec 07
  ↓ Claude architect
Task 07 仕様確定
  ↓
/impl 07
  ↓ 実装プロンプト
Codex
  ↓ make verify + Implementation Record
/review 07
  ↓ Claude reviewer
CHANGES REQUESTED ──→ /fix 07 ──→ Codex ──→ /review 07
  ↓ READY
done
  ↓
人間がコミット
```

## 初回導入

1. この scaffold のファイルを既存 ops-hub にマージする。
2. 既存 `justfile`、`docs/implementation-plan.md`、Task 01〜06文書を置換しない。
3. `chmod +x .claude/hooks/format-rust.sh .githooks/commit-msg` を確認する。
4. 必要なら `make install-git-hooks` を実行する。
5. ローカルDBを既存手順で起動し `DATABASE_URL` を設定する。
6. `make verify` を実行する。
7. `make task-index` で16タスク一覧を生成する。
8. Task 07から `/spec 07` を使用する。
