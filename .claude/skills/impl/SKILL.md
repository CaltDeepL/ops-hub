---
name: impl
description: 承認済みops-hub taskをCodexへ渡すための日本語実装プロンプトを生成する。Claude自身は実装しない。
argument-hint: "ID"
disable-model-invocation: true
---

Task `$0` のCodex handoffを作る。

1. `docs/task-$0-*.md`、`AGENTS.md`、`docs/ai/PROJECT.md`、`docs/ai/WORKFLOW.md` を読む。
2. managed taskでは `status: spec` であること、未解決のSpec Deviationsがないことを確認する。
3. 設計が曖昧なら実装プロンプトを作らず `/spec $0` に戻す。
4. task statusだけを `implementing` へ変更する。
5. `make task-index` を実行する。
6. 次の内容を含むCodex用日本語プロンプトを生成する。

```text
<task-path> を実装してください。

最初に AGENTS.md、task doc、taskから参照される文書と関連コード/test/migrationを読んでください。
設計判断・不変条件（DD-*）を守り、今回のtaskに必要な最小差分だけを実装してください。

DD-* と矛盾する変更が必要だと判明した場合は、実装で勝手に変更せず、Spec Deviationsへ理由を記録し status: blocked にして、その経路の作業を止めてください。

実装中は必要に応じて狭いtestを実行して構いません。
最終品質ゲートは必ず make verify です。
query/migration変更でSQLx metadataが変わる場合は make sqlx-prepare で .sqlx/ を更新してください。

Neon本番DBへ接続しないでください。.envの秘密情報を読まないでください。
git commit / rebase / push / deploy は行わないでください。

完了時にAcceptance Criteriaを実際の結果に合わせて更新し、Implementation Recordへ変更ファイル、判断、DB/API影響、make verifyの実結果、残課題を記録してください。

7. pbcopy / wl-copy / xclip が既に存在する場合のみclipboardへコピーする。インストールしない。

8. production codeは編集しない。
