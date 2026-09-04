# タスク02：マイグレーション0001（ENUM・targets・target_states）

| 項目 | 内容 |
|---|---|
| 上位ドキュメント | ops-hub-detail v1.0 10章 タスク2 |
| 完了条件 | 適用と revert→run の往復が通る |
| ステータス | 完了（`sqlx migrate revert` → `run` の往復と検証13項目を実機確認） |

## 1. このタスクで作ったもの

| ファイル | 内容 |
|---|---|
| `migrations/0001_init.up.sql` | ENUM 7種、`set_updated_at()`、`targets`、`target_states`、トリガ2種 |
| `migrations/0001_init.down.sql` | 上記を逆順に落とす |
| `scripts/verify_0001.sql` | 制約・トリガの挙動確認スクリプト（`psql` で直接流す。**`migrations/` には置かない**） |

ファイル名を `0001_init.up.sql` / `0001_init.down.sql` としているのは、sqlx が
`{version}_{description}.{up|down}.sql` を解釈するため。`sqlx migrate add -r` で作ると
タイムスタンプ版になるので、基本設計 2.2 の命名に合わせて手で置いている。

## 2. 設計判断（基本設計に無い追加が2件ある）

| # | 判断 | 根拠 |
|---|---|---|
| D-10 | `targets` への INSERT で `target_states` を自動生成するトリガを置く | 1:1 の関係をDB側で保証する。アプリの入れ忘れで「有効な監視対象なのに状態行が無い」状態を作らせない。D-4・D-5と同じ「アプリのバグに依存しない」方針の延長 |
| D-11 | `target_states` にも `set_updated_at()` トリガを張る | 状態更新のたびにアプリが `updated_at` を書く手間を消す。設定側の `updated_at` を汚さないことは 2.4 の分離で担保済み |
| — | ENUM 7種すべてを0001に置く | 基本設計 2.2 のとおり。型を先に確定させると 0002〜0004 を独立して revert できる |
| — | `COMMENT ON` を付与 | `\d+` と将来のER図生成で、列の意図（90秒の根拠など）が設計書を開かずに追える |

D-10・D-11 は設計書に無い変更なので、採否の判断が要る。不要なら該当の
`CREATE FUNCTION create_target_state` / `CREATE TRIGGER` を削るだけで済む。

## 3. 検証結果（PostgreSQL 16.15、ローカル実接続）

`up → down → up` の往復が通り、down 後に public スキーマへ残るテーブル・ENUM・関数は0件。

| # | 検証項目 | 結果 |
|---|---|---|
| 1 | targets 追加で target_states が自動生成（`up` / 失敗数0） | OK |
| 2 | 大文字違いの同名を拒否（`targets_name_key`） | OK 23505 |
| 3 | `https://` 以外のURLを拒否 | OK 23514 |
| 4 | GET/HEAD 以外のメソッドを拒否 | OK 23514 |
| 5 | `timeout_ms` の下限（1000未満）を拒否 | OK 23514 |
| 6 | `timeout_ms` の上限（120000超）を拒否 | OK 23514 |
| 7 | `degraded_threshold_ms >= timeout_ms` を拒否 | OK 23514 |
| 8 | 空白のみの名前を拒否 | OK 23514 |
| 9 | 未定義の severity を拒否 | OK 22P02 |
| 10 | 負の `consecutive_failures` を拒否 | OK 23514 |
| 11 | 状態更新で `targets.updated_at` が動かない | OK |
| 12 | 設定更新で `targets.updated_at` が動く | OK |
| 13 | targets 削除で target_states が CASCADE 削除 | OK |

> 検証環境は 16.15。本番は Neon の 17 だが、使用しているのは 13以降で共通の機能のみ
> （組み込みの `gen_random_uuid()`、部分索引、式索引、plpgsql トリガ）。
> 2〜9 で観測した SQLSTATE は、タスク4の `AppError` マッピングでそのまま使える。

## 4. 初期データ（seed）の扱い

要件 F-1 は「初期投入はマイグレーション内のINSERT」としているが、0001 には入れていない。

- 監視対象のURLが未確定（チェスアプリのバックエンド／フロント、資産管理APIは本番デプロイ後）
- `targets_url_https` があるため、仮のURLを入れると通らない
- スキーマとデータを別マイグレーションに分けたほうが、対象追加のたびに履歴が読める

URL確定後に `0005_seed_targets.up.sql` として追加する。D-10 のトリガがあるので、
`target_states` の INSERT は不要。

```sql
INSERT INTO targets (name, url, severity) VALUES
  ('chess-api',   'https://<ここを埋める>/health', 'sev2'),
  ('chess-web',   'https://<ここを埋める>/',       'sev2'),
  ('asset-log',   'https://<ここを埋める>/health', 'sev1');
```

down 側は `DELETE FROM targets WHERE name IN (...)`。`checks` が参照を持つと
`ON DELETE RESTRICT` で消せなくなるため、seed の revert は初回投入直後だけ有効。

## 5. つまずいた点と教訓

| # | 事象 | 原因 | 対処 |
|---|---|---|---|
| 1 | `migration 1 was previously applied but has been modified` | 検証用SQLを `migrations/` に置いてしまい、正式な `0001_init` と同じバージョン番号として読まれた。sqlx は `migrations/` 配下のファイル名先頭を整数バージョンとして解釈する | `scripts/verify_0001.sql` へ退避。**`migrations/` にはマイグレーション以外を置かない**を運用ルールにする |
| 2 | `sqlx database drop` が失敗する | api コンテナが接続を保持していた。PostgreSQL は接続中のDBを落とせない | api を停止 → `drop -y` → `create` → `migrate run` → api 再起動 |
| 3 | `psql: /tmp/.s.PGSQL.5432 に接続できない` | `DATABASE_URL` が未設定で、psql がローカルのUnixソケットにフォールバックしていた | `.env` に host 用の接続文字列を記載。psql を直接叩くときは `set -a && source .env && set +a`（sqlx-cli は `.env` を自動で読む） |

**教訓：`DATABASE_URL` はホスト用（`localhost:5433`）とコンテナ間用（`db:5432`）で値が違う。**
ホスト側の公開ポートは他のPostgreSQLとの競合を避けて5433にずらしてあるが、コンテナ間通信は
Docker の内部ネットワークを使うため 5432 のまま。ここを取り違えると、**ローカルで動いている
別のPostgreSQLにマイグレーションを流し込む**事故になる。実行前に必ず接続先を確かめる。

## 6. 再現コマンド

```bash
# sqlx-cli（未導入なら）
cargo install sqlx-cli --no-default-features --features rustls,postgres

docker compose up -d db

# ホストから繋ぐときのポートは 5433（コンテナ間の db:5432 とは別物）
set -a && source .env && set +a        # DATABASE_URL=postgres://ops_hub:ops_hub@localhost:5433/ops_hub

# ★実行前の安全確認：ops_hub が返らなければ絶対に流さない
psql "$DATABASE_URL" -c "select current_database()"

sqlx migrate run                       # 適用
sqlx migrate info                      # 1/installed init
psql "$DATABASE_URL" -f scripts/verify_0001.sql   # 13項目がすべて OK

sqlx migrate revert                    # 巻き戻し
# 残存が _sqlx_migrations のみであることの確認
psql "$DATABASE_URL" -c "\dt" -c "\dT" -c "\df"

sqlx migrate run                       # 再適用（往復の確認）
```

実測（2026-09-05）：`revert` 後に残ったテーブルは `_sqlx_migrations` の1件のみ、
public スキーマの ENUM・関数はいずれも0件。`run` で `1/installed init`。

## 7. 次タスクへの引き継ぎ

- タスク3で 0002〜0004（`runs` / `checks` / `check_dailies` / `incidents` / `events` / `outbox`）
- ENUM は0001で作成済みのため、0002以降は型定義を書かない
- `incidents` の部分ユニーク索引（D-5）と `outbox.dedupe_key` の UNIQUE（D-4）は、
  0001 と同じやり方で「本当に2件目が弾かれるか」を実接続で確認してから完了とする
- `outbox_sent_has_timestamp` は `(status = 'sent') = (sent_at IS NOT NULL)` という
  等価CHECK。`failed` 時に `sent_at` を入れていないかも検証項目に含める