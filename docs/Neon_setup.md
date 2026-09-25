# Neon セットアップ（ops-hub）

| 項目 | 内容 |
|---|---|
| 目的 | マイグレーション適用先、および Render 本番の接続先となる Neon を作成し、詳細設計11章の宿題1〜3を検証する |
| 完了条件 | 直接エンドポイントで接続でき、`scripts/verify-neon.sh` の検証項目が期待どおりになる |
| 関連 | Task 05（宿題の検証結果）/ `Render_setup.md` |
| ステータス | 完了（リージョン `ap-southeast-1`、PostgreSQL 17.11。宿題1〜3は Task 05 で閉じた） |

## 1. 実施内容（手順）

### 1.1 プロジェクト作成

Neon Console → **New Project**

| 項目 | 値 | 備考 |
|---|---|---|
| Project name | `ops-hub` | |
| Postgres version | **17** | `compose.yaml` の `postgres:17` と揃える。ローカルで通って Neon で落ちる、を減らす |
| Region | 2章 決定B のとおり | **変更不可** |
| Database name | `ops_hub` | 既定の `neondb` のままにしない |
| Role name | `ops_hub` | 既定は `neondb_owner` |

作成後、**パスワードは一度しか表示されない**。この時点で控えておく。

ブランチは `main`（Render 本番）のみ。開発ブランチは作らない（2章 決定C）。

### 1.2 接続文字列（直接エンドポイントを使う）

Connection Details の **Connection pooling** トグルを **OFF** にした文字列を使う。

```
# 正しい（直接）
postgresql://ops_hub:****@ep-xxxx-yyyy-123456.<region>.aws.neon.tech/ops_hub?sslmode=require

# 誤り（プール済み）— ホスト名に -pooler が入っている
postgresql://ops_hub:****@ep-xxxx-yyyy-123456-pooler.<region>.aws.neon.tech/ops_hub?sslmode=require
```

`?sslmode=require` は必須。sqlx は `tls-rustls` でビルドしてある。

### 1.3 検証（宿題1〜3）

| # | 検証 | 期待 | 外れた場合 |
|---|---|---|---|
| 1 | ホスト名に `-pooler` を含まない | 含まない | **即中止。** 接続文字列を取り直す |
| 2 | `pg_try_advisory_lock` が2セッションで競合し、`unlock` 後に再取得できる | 競合する | プーラ経由を疑う。`RunLock` の前提が崩れる |
| 3 | `pg_locks` にキーが見える | 1行見える | 脱出ハッチ（Task 07）が作れない |
| 4 | 宿題1: `SHOW tcp_keepalives_idle` が実値を返す | 非ゼロ（実測は `60`） | — |
| 5 | 宿題1: `SET tcp_keepalives_idle = 30` が通る | 通る | 接続文字列の `options` を試す。それも駄目なら3章 (c) の判断へ |
| 6 | 宿題2: 自ロールの別セッションを `pg_terminate_backend` できる | `t` | 脱出ハッチを諦め、`already_running` にフォールバック（詳細設計 1.3 の但し書き） |
| 7 | `max_connections` | 100 前後（0.25 CU で 104） | プールの `DB_MAX_CONNECTIONS=5` は十分小さい |

結果は `Task_05_run_lock.md` の1章「宿題の消化」に反映済み。

### 1.4 Render 側への設定

Render の Environment に設定する。**Render の内部 DB ではなく Neon を指す。**

| 変数 | 値 |
|---|---|
| `DATABASE_URL` | Neon の直接エンドポイント（`-pooler` なし） |
| `RUN_LOCK_KEY` | `8421337`（既定のままなら未設定でよい） |
| `DB_MAX_CONNECTIONS` | `5` |
| `RUST_LOG` | `info,sqlx=warn,tower_http=info` |

起動ログに `-pooler` 警告が出ていないことを、デプロイのたびに確認する。

### 1.5 記録しておくこと

- プロジェクト ID / エンドポイント ID（`ep-...`）
- リージョンと、それを選んだ理由（決定B）
- ロールのパスワードの保管先（1Password 等。リポジトリには置かない）

## 2. 設計判断

### 決定A: 既存プロジェクトに相乗りせず、ops-hub 専用プロジェクトを作る

Neon の「プロジェクト」は compute（Postgres プロセス）の単位。既存の asset-tracker のプロジェクトに `ops_hub` データベースを足す選択肢もあるが、採らない。

理由は**障害の巻き添え**。ops-hub は asset-tracker を監視する側なので、compute を共有すると「監視対象が落ちる原因」と「監視する側が落ちる原因」が同一になる。asset-tracker の DB が不調なとき、ops-hub もインシデントを記録できず通知も飛ばせない。**監視が必要なときに限って監視が死ぬ**構成になる。

副次的な利点として、`checks` の書き込みが asset-tracker の compute 時間を食わない。コストは、無料枠のストレージとプロジェクト数を1つ余分に使うこと。

> advisory lock のスコープは**データベース単位**なので、排他制御の観点だけなら同一プロジェクト内の別データベースでも正しく動く。分ける理由は上記の可用性のみ。

### 決定B: リージョンは Render のサービスと同じところに置く

Neon はプロジェクト作成後にリージョンを変更できない（移すにはダンプ&リストアが要る）。Render に東京リージョンは無いので、**東京（ap-northeast-1）を選ばない**。Render 側が Singapore なら `AWS ap-southeast-1`、Oregon なら `AWS us-west-2`。

判断基準は「1回の run で ops-hub が Neon に何往復するか」。対象3件でも `runs` / `checks` / `target_states` / `incidents` / `outbox` への書き込みで十数往復は出るので、太平洋を跨ぐと1往復100ms 級のコストが効く。GitHub Actions（実行トリガ）との距離は、リクエストが1回だけなので無視してよい。

### 決定C: 開発ブランチを切らない

ローカルに DB がある以上 Neon を開発で叩く理由が無く、無料枠のストレージと compute 時間を本番専用に温存する。

### 決定D: 直接エンドポイントのみを使う

Neon の pooler は PgBouncer の transaction モードで動いており、**セッションレベルの advisory lock は公式にサポート対象外**。`-pooler` を掴むと `RunLock::try_acquire` が毎回 `true` を返して二重実行を止められなくなる。例外もエラーも出ない壊れ方をするので、Task 01 で入れた起動時の `-pooler` 警告が最後の砦になる。

## 3. つまずいた点と教訓

### 取り違え：ローカルと Neon

ローカルは `localhost:5433`、Neon は `ep-....neon.tech`。`sqlx migrate run` を叩く前に `psql ... -c "select current_database()"` を挟む習慣にする。Task 01 でホストの 5432 に別の Postgres が居たことがあり、他人の DB にテーブルを作る事故は実際に起こりうる。

### scale to zero と ops-hub の実行モデル

Neon 無料プランは**5分間クエリが無いと compute が停止**し（無効化不可）、次の接続で数百 ms かけて起動する。停止時にセッションは全て切れる。ops-hub への影響は3点。

- **(a) 実行間隔60分なので、毎回コールドスタートになる。** `connect_lazy` で起動自体はブロックしないが、run の最初のクエリに数百 ms 乗る。タイムアウト設計には影響しない規模
- **(b) run と run の間にプールの接続は死ぬ。** sqlx は既定で `test_before_acquire` が有効なので、死んだ接続は取得時に捨てられて張り直される。設定変更は不要
- **(c) ロック保持コネクションは「アイドル」である。** compute の停止判定はコネクション単位ではなく compute 単位で、run の最中は他のコネクションがクエリを流し続けているため、実行中に停止することはない。**run が5分を超えて、かつその間 DB へのクエリが一切無い場合に限り**、compute が停止してロックが消える。現行モデルでは起こらないが、対象数が増えて run が長時間化したら再検討する

(c) が示すとおり、**最悪ケースでもロックは「残る」のではなく「消える」**。次回以降を永久にブロックする方向には壊れないので、advisory lock を選んだ判断（D-2）は Neon 上でも正しい。

### Unix ソケットでは `tcp_keepalives_idle` が 0 に見える

Task 05 で判明。宿題1が一度「判定不能」になった原因。TCP 接続で確認すること。

## 4. 再現コマンド

```bash
# 宿題1〜3の検証
export DATABASE_URL="postgresql://ops_hub:****@ep-....aws.neon.tech/ops_hub?sslmode=require"
./scripts/verify-neon.sh

# マイグレーションの適用（直接エンドポイントであることを毎回確かめる）
psql "$DATABASE_URL" -At -c "select current_database(), current_user;"
# ops_hub|ops_hub が返ること

cargo install sqlx-cli --no-default-features --features rustls,postgres  # 未導入なら
sqlx migrate info
sqlx migrate run
```

> 注：`scripts/verify-neon.sh` は現在のリポジトリには存在しない（Task 18 の復元対象外）。

## 5. 次タスクへの引き継ぎ

- Render の作成と `DATABASE_URL` の設定は `Render_setup.md` へ
- 対象数が増えて run が5分を超えるようになったら、3章 (c) の compute 停止によるロック消失を再検討する
