# ops-hub — Claude Code 運用ルール v3

共通ルールの正本は `AGENTS.md`。Claude Code は Architect / Context Compiler として仕様化と独立reviewを担当し、実リポジトリへの実装はCodexへ渡す。

## 標準フロー

```text
/spec ID
  ↓ Claude: DRAFTを作成
Human Gate
  ↓ Human: APPROVED
/impl ID
  ↓ Codex: IMPLEMENTED
/review ID
  ├─ READY → Humanがcommit/pushしDONE
  ├─ FIX → statusはIMPLEMENTEDのまま /fix ID
  └─ BLOCKED → Human/architect判断
```

`DRAFT → APPROVED`と`READY → DONE`はHumanだけ、`APPROVED → IMPLEMENTED`はCodexだけ、`IMPLEMENTED → READY`はClaudeだけが行う。

## `/spec` の成果物

- `docs/task-ID-*.md`: Goal / Scope / Out of Scope / Invariants / Files / Required Tests / Acceptance Criteria / Design Decisions / Likely Pitfalls / Verifyを具体化する。
- `docs/commits/task-ID.txt`: Humanが使う推奨commit message。Claude/Codexはcommitしない。
- `MANIFEST.md`: 変更予定pathと目的だけを記す一時ファイル。正しさの基準ではなく、原則commitしない。
- `implementation/`: 新規・自己完結・弱結合で既存参照が2〜3箇所以内の場合だけ作成可。未検証の提案であり、Codexは破棄・再実装してよい。

HumanはGoal / Scope / Out of Scope、Invariants、Acceptance Criteria、MANIFESTの変更範囲を確認してからstatusを`APPROVED`へ変更する。Claudeは自分で承認しない。

## Review

Humanから次の順で渡された材料をdiff-firstで照合する。

1. Human承認済みAcceptance Criteria
2. 実際のdiff
3. Required Testsと実行結果
4. Invariants
5. Implementation Recordの`make verify`証拠
6. task docのDesign Decisions
7. 判断不能時だけ該当ADR・上位設計の必要箇所

Claude生成コードやMANIFESTを正解として循環参照しない。判定は`READY / FIX / BLOCKED`のみ。FIXではstatusを変更せず、READYの場合だけ`IMPLEMENTED → READY`にする。Human ACをAIが完了扱いにしない。

## 読み取りとagent routing

通常は対象task、MANIFEST、diff、対象コード・testだけを読む。`PROJECT.md`、`WORKFLOW.md`、全ADR、全設計文書を最初から読まない。L1/L2/L3の条件は`AGENTS.md`に従う。

- architect: migration/schema、concurrency、状態遷移、idempotency、API互換性、CI/failure domainの設計整理。read-only。
- reviewer: 通常diffの独立review。read-only。
- reviewer-critical: migration、lock/transaction/idempotency、重要な状態遷移、BLOCKER再確認。read-only。
- debugger: compile/test/SQLx/PostgreSQL/Docker/runtime/CIの原因調査。read-only。

## 品質・安全境界

- 最終品質ゲートは`make verify`。`make audit`は別の依存脆弱性検査。
- SQLx/DB操作はMake targetとlocal DB guard経由だけ。直接SQLx、`psql`、CLIでの`DATABASE_URL`上書きを行わない。
- `.env`やsecretを読まない。本番DBへ接続しない。
- commit/push/rebase/reset/clean/tag/release/merge/deployを行わない。
