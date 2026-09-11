---
name: impl
description: 承認済みops-hub taskをCodexへ渡すための日本語実装プロンプトを生成する。Claude自身は実装しない。
argument-hint: "ID"
disable-model-invocation: true
---

Task `$0` のCodex handoffを作る。

1. `AGENTS.md`、`docs/task-$0-*.md`、存在する場合だけ`MANIFEST.md`を読む。全ADR・上位設計は再読しない。
2. managed taskでは `status: APPROVED` であること、未解決のSpec Deviationsがないことを確認する。Human Gate未通過なら停止する。
3. 設計が曖昧なら実装プロンプトを作らず `/spec $0` に戻す。
4. statusは`APPROVED`のままCodexへ渡す。
5. 次の内容を含むCodex用日本語プロンプトを生成する。

```text
<task-path> を実装してください。

L0として承認済みtask doc、MANIFEST、Filesの対象、関連testだけを読んでください。
Acceptance Criteria、Invariants、Out of Scopeを守り、今回のtaskに必要な最小差分だけを実装してください。MANIFESTやimplementation/は未検証の提案なので、実リポジトリと衝突する場合は破棄してください。

不足時だけL1（呼び出し元・関連test・直接依存module）、設計衝突の疑いがある場合だけL2（該当ADR・上位設計の該当箇所）へ広げてください。L2到達理由は記録してください。L2でも一意でなければL3として停止してください。

Invariantsと矛盾する変更が必要だと判明した場合は、実装で勝手に変更せず、Spec Deviationsへ理由を記録し status: BLOCKED にして、その経路の作業を止めてください。同一原因への修正が3回失敗した場合もBLOCKEDにしてください。

実装中は必要に応じて狭いtestを実行して構いません。
最終品質ゲートは必ず make verify です。
query/migration変更でSQLx metadataが変わる場合は make sqlx-prepare で .sqlx/ を更新してください。cargo sqlx、sqlx migrate、psqlを直接実行せず、DATABASE_URLをコマンドラインで上書きしないでください。

Neon本番DBへ接続しないでください。.envの秘密情報を読まないでください。
git commit / rebase / push / reset / clean / tag / release / merge / deploy は行わないでください。

完了時にAcceptance CriteriaのCodex項目だけを実際の結果に合わせて更新してください。Human項目は更新しません。Implementation RecordはChanged / Decision / Impact / Verify / Remainingの固定形式で短く記録し、全条件を満たした場合だけstatusをIMPLEMENTEDにしてください。成功ログはcommand、exit code、PASS、最終行だけに要約してください。
```

6. pbcopy / wl-copy / xclip が既に存在する場合のみclipboardへコピーする。インストールしない。

7. Claude自身はproduction codeを編集しない。
