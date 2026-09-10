# Render セットアップ（ops-hub）

Neon 作成後の手順。`render.yaml` を Blueprint として反映し、GitHub Actions（タスク15）から
叩ける公開 URL を用意するところまで。

---

## 1. 前提の確認

| 項目 | 値 |
|---|---|
| Neon リージョン | `ap-southeast-1`（適用済み） |
| Render リージョン | **`singapore`**（同じ AWS リージョン） |
| プラン | Free |
| ランタイム | Docker（リポジトリの `Dockerfile`） |

Render に東京は無いので、Neon を Singapore に置いた判断とセットで固定する。
**リージョンはサービス作成後に変更できない。**変えるにはサービスを作り直す。

---

## 2. 事前に入れるコード変更（`/livez`）

`render.yaml` の `healthCheckPath` は `/livez` を指している。この時点では存在しないので、
Blueprint を反映する前に追加する。理由は4章。

`src/handler/livez.rs`

```rust
//! liveness 用。**DBに触らない。**
//!
//! プラットフォーム（Render）の再起動判断に使う。プロセスが生きていれば 200 を返す。
//! DB疎通まで見る `/health` とは目的が違う（4章）。

use axum::http::StatusCode;

pub async fn livez() -> (StatusCode, &'static str) {
    (StatusCode::OK, "ok")
}
```

`src/handler/mod.rs`

```rust
 pub mod health;
+pub mod livez;
```

`src/lib.rs`

```rust
 pub fn app(state: AppState) -> Router {
     Router::new()
         .route("/health", get(handler::health::health))
+        .route("/livez", get(handler::livez::livez))
         .layer(TraceLayer::new_for_http())
         .with_state(state)
 }
```

---

## 3. Blueprint の反映

1. `render.yaml` をリポジトリのルートに置いて `main` に push する
2. Render ダッシュボード → **Blueprints** → **New Blueprint Instance**
3. リポジトリを選び、ブランチ `main` を指定して **Apply**
4. `sync: false` の3件（`DATABASE_URL` / `SLACK_WEBHOOK_URL` / `SERVICE_TOKENS`）は
   値の入力を求められる。この時点では `DATABASE_URL` だけ入れ、残りは空のままでよい
   （タスク11・12で埋める）

`DATABASE_URL` は Neon の**直接エンドポイント**。`-pooler` が入っていないことを
貼り付ける前に目視する。起動ログに `-pooler` 警告が出ていないことでも確認できる。

### 初回デプロイの確認

```bash
curl -i https://ops-hub.onrender.com/livez     # 200 ok（DBに触らない）
curl -i https://ops-hub.onrender.com/health    # 200 {"status":"ok",...,"db":"up"}
```

`/health` が 503 を返す場合は `DATABASE_URL` が誤っている。ログの
`database = postgres://ops_hub:***@...` を見てホストを確認する。

---

## 4. なぜ `healthCheckPath` を `/health` にしないのか

`/health` は **DB疎通まで確認して、到達不能なら 503 を返す**設計（タスク1）。これは
readiness の意味づけであって、liveness ではない。

Render は `healthCheckPath` が失敗し続けるインスタンスを異常とみなして再起動する。
`/health` を指定すると、**Neon 側の一時的な不調でアプリのプロセスが殺される**。
ops-hub は「監視する側」なので、監視対象や依存先が不調なときこそ生き残って
インシデントを記録し通知を出す必要がある。ここで落ちると設計の意図が反転する。

- `/livez` … プロセスが生きているか。Render の再起動判断に使う
- `/health` … DBまで含めて機能しているか。デッドマンスイッチと運用当番が見る

`/health` を捨てるわけではない。**見る主体が違うので分ける。**

---

## 5. 無料枠（750インスタンス時間/月）の見積もり

要件定義 v0.2 で案Bを選んだ根拠を、実際の数字で確認しておく。

Render Free は**15分間 inbound が無いとスピンダウン**し、次のリクエストで
約1分かけて起動する。ops-hub は60分間隔なので、1回の run につき

```
起動 約1分 + スピンダウンまでの待機 15分 ≒ 16分
```

がインスタンス時間として計上される。

| 項目 | 計算 | 値 |
|---|---|---|
| 1日 | 16分 × 24回 | 約 6.4 時間 |
| 1か月 | 6.4 × 30 | **約 192 時間** |
| 残り | 750 − 192 | 約 558 時間 |

750時間は**ワークスペース単位**で、asset-tracker の Web Service と共有する。
残り558時間を asset-tracker が使う形になるので、asset-tracker が常時起動していると
枯渇する。asset-tracker 側もスピンダウンする Free プランなら問題ない。

> 枯渇すると Free の Web Service は翌月まで停止する。ops-hub が止まると
> GitHub Actions のワークフローが失敗し、デッドマンスイッチとしてメールが届く。
> **枯渇そのものは検知できる設計になっている**が、原因が「無料枠切れ」だと
> 気づくまで時間を食うので、Render の Usage を運用当番の確認項目に入れる。

---

## 6. デプロイのトリガ

`autoDeploy: false` にしてある。asset-tracker（#24）と同じ「CI green → Deploy Hook」に
揃えるため。

1. Render ダッシュボード → Settings → **Deploy Hook** の URL をコピー
2. GitHub リポジトリの Secrets に `RENDER_DEPLOY_HOOK_URL` として登録
3. `ci.yml` の成功後に `curl -fsS "$RENDER_DEPLOY_HOOK_URL"` を叩く

タスク16（CI）で実装する。それまでは Render ダッシュボードの **Manual Deploy** で足りる。

> #16 で踏んだ `paths` フィルタの罠に注意。`ci.yml` の `paths` に対象外の
> ディレクトリしか変えていない PR は CI がスキップされ、Deploy Hook も飛ばない。

---

## 7. ビルド時間（Rust + Docker の宿題）

現在の `Dockerfile` は `COPY . .` してから `cargo build` する構成なので、
**ソースを1行変えるたびに依存クレートを全部ビルドし直す。** axum + sqlx + reqwest の
フルビルドは数分〜10分かかる。Render のパイプライン時間を無駄に食う。

タスク16でまとめて対処する。方針は依存だけを先にビルドするレイヤを作ること
（`cargo-chef`、またはダミーの `src/main.rs` を置いて `cargo build` する手法）。

```dockerfile
# 方針のイメージ（タスク16で実装する）
FROM rust:1.96-slim-bookworm AS planner
WORKDIR /app
RUN cargo install cargo-chef --locked
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM rust:1.96-slim-bookworm AS builder
WORKDIR /app
RUN cargo install cargo-chef --locked
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json   # ここがキャッシュされる
COPY . .
RUN cargo build --release
```

初回デプロイを通すのが先なので、**今は現行の Dockerfile のままでよい。**

---

## 8. 次にやること

| 順序 | 内容 | タスク |
|---|---|---|
| 1 | `/livez` を追加して push | 本ドキュメント2章 |
| 2 | Blueprint 反映・`DATABASE_URL` 設定・`/livez` と `/health` の疎通確認 | 本ドキュメント3章 |
| 3 | `POST /v1/runs` の実装 | タスク6 |
| 4 | GitHub Actions から 202 を確認 | タスク15 |

宿題4（Render のリクエストタイムアウト実測、`--max-time 120` の妥当性）は
タスク15で確認する。`POST /v1/runs` は 202 を即返す設計（D-1）なので、
実質の待ち時間はスピンアップの約1分。120秒は妥当な線に見えるが、実測で裏を取る。