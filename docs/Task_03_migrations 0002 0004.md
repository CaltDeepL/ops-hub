# タスク03：マイグレーション0002〜0004

| 項目 | 内容 |
|---|---|
| 上位ドキュメント | ops-hub-detail v1.0 10章 タスク3 |
| 完了条件 | 適用と revert→run の往復が通る。全テーブルの制約・索引が意図どおり生成されている |
| ステータス | SQL検証済み（PostgreSQL 16.15）／ 手元での sqlx-cli 往復は未実施 |

## 1. このタスクで作ったもの

| ファイル | 内容 |
|---|---|
| `migrations/0002_runs_checks.up/down.sql` | `runs` / `checks` / `check_dailies` |
| `migrations/0003_incidents.up/down.sql` | `incidents`（部分ユニーク索引つき） |
| `migrations/0004_events_outbox.up/down.sql` | `events` / `outbox` |
| `scripts/verify_0002_0004.sql` | 制約・索引の挙動確認（**`migrations/` には置かない**） |

ENUM は0001で作成済みのため、0002以降では型を定義していない。
このため 0002〜0004 は個別に revert しても ENUM が巻き添えにならない。

## 2. 設計書からの追加（判断が要るもの）

| # | 追加 | 根拠 |
|---|---|---|
| A | `runs_counts_non_negative` | `targets_checked` / `notifications_sent` に負値が入る経路は本来ないが、集計のバグを黙って通さない |
| B | `runs_running_idx`（`status='running'` の部分索引） | スイーパー（基本設計3.4）と「実行中の run_id を引く」（詳細設計2.2）が毎回叩く。running は高々1件なので索引は極小 |
| C | `checks_started_at_idx` | N-14 の90日削除（詳細設計5.3）が `started_at` 単体で範囲削除する。既存の複合索引は先頭が `target_id` なので効かない |
| D | `check_dailies_degraded_le_success` | `degraded` は成功チェックの部分集合（基本設計4.2）。集計SQLの `FILTER` を書き間違えたら気づけるようにする |
| E | `check_dailies_percentiles_non_negative` | 同上。`percentile_cont` の対象を誤ると負値やNULLの扱いがずれる |
| F | `events_source_not_blank` / `events_idempotency_key_not_blank` | 空文字の冪等キーは「全リクエストが同一キー」と同義になり、通知が1回しか出なくなる |
| G | `outbox_failed_idx`（`status='failed'` の部分索引） | 運用当番（O-2）が毎週「諦めた通知」を見る |
| H | `outbox_dedupe_key_not_blank` | 空文字だと UNIQUE が「全通知で1件だけ」を意味してしまう |

いずれも既存の設計判断（D-4・D-5の「アプリのバグに依存せずDBで防ぐ」）の延長で、
削っても機能は変わらない。不要なら該当行を消すだけで済む。

## 3. 検証結果（PostgreSQL 16.15、ローカル実接続）

`0001→0004` の適用、`0004→0001` の revert、再適用の往復が通る。
revert 後に public スキーマへ残るテーブル・シーケンス・ENUM・関数は0件。再適用後のテーブルは8件。

| # | 検証項目 | 結果 |
|---|---|---|
| 1 | `finished_at < started_at` を拒否 | OK 23514 |
| 2 | 負のチェック件数を拒否 | OK 23514 |
| 3 | 負の `duration_ms` を拒否 | OK 23514 |
| 4 | `error_detail` 513文字を拒否 | OK 23514 |
| 5 | 範囲外の `status_code` を拒否 | OK 23514 |
| 6 | `checks` が参照中の `targets` の削除を拒否（RESTRICT） | OK 23503 |
| 7 | `success_count > total_count` を拒否 | OK 23514 |
| 8 | `degraded_count > success_count` を拒否 | OK 23514 |
| 9 | 日次集計の UPSERT が1行に収束 | OK |
| 10 | 未解決インシデントの2件目を拒否（D-5） | OK 23505 |
| 11 | `resolved_at <= started_at` を拒否 | OK 23514 |
| 12 | 復旧後は次のインシデントを開ける | OK |
| 13 | 同一 `source` + 冪等キーの重複を拒否 | OK 23505 |
| 14 | `title` 201文字を拒否 | OK 23514 |
| 15 | 別 `source` なら同じ冪等キーを使える（D-9） | OK |
| 16 | `dedupe_key` の重複を拒否（D-4） | OK 23505 |
| 17 | `attempts = 6` を拒否 | OK 23514 |
| 18 | `sent` なのに `sent_at` が無い行を拒否 | OK 23514 |
| 19 | `pending` なのに `sent_at` がある行を拒否 | OK 23514 |
| 20 | `status` と `sent_at` を同時更新すれば通る | OK |
| 21 | 索引11件がすべて生成されている | OK |

**タスク4への直結点**：10・13・16 で観測した 23505 は、詳細設計 3.3 の分類表で
扱いが3通りに分かれる（`events_source_idempotency_key` と `outbox_dedupe_key` は正常系、
`incidents_one_open_per_target` は `Internal`）。制約名で分岐するテストがそのまま書ける。

`outbox_sent_has_timestamp` は等価CHECKなので、18（sent なのに timestamp 無し）と
19（pending なのに timestamp 有り）の**両方向**が弾かれることを確認済み。
配信処理では `status` と `sent_at` を必ず同じ UPDATE で書く。

## 4. つまずいた点と教訓

（sqlx-cli で流して埋める。特に `checks.id` の `bigserial` が down で
シーケンスごと消えるか＝所有関係が切れていないかは、手元でも確認しておきたい）

## 5. 再現コマンド

```bash
set -a && source .env && set +a
psql "$DATABASE_URL" -c "select current_database()"   # ★ ops_hub が返ること

sqlx migrate run
sqlx migrate info                                     # 1〜4 が installed
psql "$DATABASE_URL" -f scripts/verify_0002_0004.sql  # 21項目がすべて OK

# 1つずつ巻き戻して、都度エラーが出ないことを見る
sqlx migrate revert   # 4
sqlx migrate revert   # 3
sqlx migrate revert   # 2
sqlx migrate revert   # 1
psql "$DATABASE_URL" -c "\dt" -c "\dT" -c "\df" -c "\ds"

sqlx migrate run
psql "$DATABASE_URL" -c "\dt"                         # _sqlx_migrations + 8テーブル
```

## 6. 次タスクへの引き継ぎ

- タスク4は `AppError` と problem+json。詳細設計3.1の enum と 3.3 のSQLSTATE分類表を実装する
- 分類の判定に**制約名**を使うため、`sqlx::Error::Database` から
  `constraint()` を取り出す `OnConstraint` 拡張トレイトを既存プロジェクトから移植する
- 5xx のレスポンスは `trace_id` のみ（N-10）。`Database` バリアントの中身をボディに出さない
- 検証スクリプトで観測した SQLSTATE と制約名の対応が、そのままテストの期待値になる

