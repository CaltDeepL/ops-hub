# ops-hub — Claude Code + Codex 併用 scaffold v3（日本語）

この scaffold は、既存の ops-hub に **上書き導入するのではなく、AI運用レイヤーだけを追加する** ための確定版です。

## v3 の前提

現行 ops-hub の正本・履歴は維持します。

- `docs/implementation-plan.md`：16タスクのロードマップ正本
- `docs/task-01-*.md` 〜 `docs/task-06-*.md`：既存タスク記録。frontmatter を後付けしない
- `docs/commits/task-01-*.txt` 〜：既存コミット文面
- `justfile`：既存DB操作用。置換しない
- `.github/workflows/monitor.yml`：存在する場合は運用監視用。品質CIと混ぜない

AI用の機械可読状態管理はQ系列と **Task 07以降** に適用します。共通ルールの正本は`AGENTS.md`です。

## 現在地点

現在地点は`docs/task-INDEX.md`を参照する。各managed taskのfrontmatterから生成されるため、このREADMEへ状態を重複記録しない。実行順はQ1 → Q2 → Q3 → Task 07を維持する。

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
│   ├── commits/
│   │   └── task-ID.txt
│   ├── ai/
│   │   ├── PROJECT.md      # 互換用の人間向け現在地
│   │   ├── WORKFLOW.md     # 互換用の短い案内
│   │   └── CI-POLICY.md    # CI固有の履歴
│   ├── adr/
│   │   └── README.md
│   └── templates/
│       ├── task-template-v3.md
│       ├── adr-template.md
│       └── commit-template.txt
└── scripts/
    ├── assert_local_database_url.py
    ├── check_task_docs.py
    ├── run_quiet.py
    └── update_task_index.py
```

## 最終検証

人間、Claude Code、Codex、将来の品質CIで最終品質ゲートを統一します。

```bash
make verify
```

個別の `cargo test --test runs_test` 等は実装中の反復用です。最終判定の代わりにはしません。

## 通常フロー

```text
/spec 07 → DRAFT
  ↓ Human Gate → APPROVED
/impl 07
  ↓ Codex + make verify → IMPLEMENTED
/review 07
  ↓ Claude reviewer
FIX ──→ /fix 07 ──→ Codex ──→ /review 07
  ↓ READY → 人間がcommit/push → DONE
```

## 初回導入

1. この scaffold のファイルを既存 ops-hub にマージする。
2. 既存 `justfile`、`docs/implementation-plan.md`、Task 01〜06文書を置換しない。
3. `chmod +x .claude/hooks/format-rust.sh .githooks/commit-msg` を確認する。
4. 必要なら `make install-git-hooks` を実行する。
5. ローカルDBを既存手順で起動する。DBコマンド時に`DATABASE_URL`をCLI上書きしない。
6. `make verify` を実行する。
7. `make task-index` で16タスク一覧を生成する。
8. Task 07から `/spec 07` を使用する。

`/spec`はInvariants・Files・Required Tests・Acceptance Criteriaをtask docへ圧縮し、一時`MANIFEST.md`とcommit message案を作る。Codexは通常L0（承認済みtask + MANIFEST + 対象file/test）だけを読み、reviewはAcceptance Criteriaとdiffを先に照合する。`make verify`は検証量を維持したまま成功ログだけを短くする。
