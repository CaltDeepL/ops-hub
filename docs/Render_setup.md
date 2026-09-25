# Render セットアップ（ops-hub）

| 項目 | 内容 |
|---|---|
| 目的 | `render.yaml` を Blueprint として反映し、GitHub Actions（タスク15）から叩ける公開 URL を用意する |
| 完了条件 | 本番 URL で `/livez` と `/health` がともに 200 を返す |
| 前提 | `Neon_setup.md` 完了（Neon は `ap-southeast-1`） |
| ステータス | 完了（Task 17 時点で Render / Neon へのデプロイ済みと記録。ただし `render.yaml` は現在のリポジトリに存在しない） |

## 1. 実施内容（手順）

### 1.1 前提の確認

| 項目 | 値 |
|---|---|
| Neon リージョン | `ap-southeast-1`（適用済み） |
| Render リージョン | **`singapore`**（同じ AWS リージョン） |
| プラン | Free |
| ランタイム | Docker（リポジトリの `Dockerfile`） |

**リージョンはサービス作成後に変更できない。** 変えるにはサービスを作り直す。

### 1.2 事前に入れるコード変更（`/livez`）

`render.yaml` の `healthCheckPath` は `/livez` を指している。Blueprint を反映する前に追加する（理由は2章）。

`src/handler/livez.rs`

```rust
//! liveness 用。**DBに触らない。**
//!
//! プラットフォーム（Render）の再起動判断に使う。プロセスが生きていれば 200 を返す。
//! DB疎通まで見る `/health` とは目的が違う。

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

### 1.3 Blueprint の反映

1. `render.yaml` をリポジトリのルートに置いて `main` に push する
2. Render ダッシュボード → **Blueprints** → **New Blueprint Instance**
3. リポジトリを選び、ブランチ `main` を指定して **Apply**
4. `sync: false` の3件（`DATABASE_URL` / `SLACK_WEBHOOK_URL` / `SERVICE_TOKENS`）は値の入力を求められる。この時点では `DATABASE_URL` だけ入れ、残りは空のままでよい（タスク11・12で埋める）

`DATABASE_URL` は Neon の**直接エンドポイント**。`-pooler` が入っていないことを貼り付ける前に目視する。

### 1.4 デプロイのトリガ

`autoDeploy: false` にしてある。asset-tracker（#24）と同じ「CI green → Deploy Hook」に揃えるため。

1. Render ダッシュボード → Settings → **Deploy Hook** の URL をコピー
2. GitHub リポジトリの Secrets に `RENDER_DEPLOY_HOOK_URL` として登録
3. `ci.yml` の成功後に `curl -fsS "$RENDER_DEPLOY_HOOK_URL"` を叩く

タスク16（CI）で実装する。それまでは Render ダッシュボードの **Manual Deploy** で足りる。

## 2. 設計判断

### Render のリージョンを Neon と揃える（singapore）

Render に東京は無い。Neon を Singapore に置いた判断とセットで固定する。

### `healthCheckPath` を `/health` ではなく `/livez` にする

`/health` は **DB 疎通まで確認して、到達不能なら 503 を返す**設計（Task 01）。これは readiness の意味づけであって liveness ではない。

Render は `healthCheckPath` が失敗し続けるインスタンスを異常とみなして再起動する。`/health` を指定すると、**Neon 側の一時的な不調でアプリのプロセスが殺される**。ops-hub は「監視する側」なので、依存先が不調なときこそ生き残ってインシデントを記録し通知を出す必要がある。

- `/livez` … プロセスが生きているか。Render の再起動判断に使う
- `/health` … DB まで含めて機能しているか。デッドマンスイッチと運用当番が見る

**見る主体が違うので分ける。**

### 無料枠（750インスタンス時間/月）で足りることを数字で確認する

Render Free は**15分間 inbound が無いとスピンダウン**し、次のリクエストで約1分かけて起動する。60分間隔なので1回の run につき「起動 約1分 + スピンダウンまでの待機 15分 ≒ 16分」が計上される。

| 項目 | 計算 | 値 |
|---|---|---|
| 1日 | 16分 × 24回 | 約 6.4 時間 |
| 1か月 | 6.4 × 30 | **約 192 時間** |
| 残り | 750 − 192 | 約 558 時間 |

750時間は**ワークスペース単位**で asset-tracker の Web Service と共有する。asset-tracker が常時起動していると枯渇するが、Free プランでスピンダウンするなら問題ない。

> 枯渇すると Free の Web Service は翌月まで停止する。ops-hub が止まると GitHub Actions のワークフローが失敗し、デッドマンスイッチとしてメールが届く。**枯渇そのものは検知できる**が、原因が「無料枠切れ」だと気づくまで時間を食うので、Render の Usage を運用当番の確認項目に入れる。

### Docker ビルドのキャッシュ最適化は後回し

現在の `Dockerfile` は `COPY . .` してから `cargo build` するので、**ソースを1行変えるたびに依存クレートを全部ビルドし直す**（数分〜10分）。依存だけを先にビルドするレイヤ（`cargo-chef` 等）で対処するが、初回デプロイを通すのが先なので**今は現行の Dockerfile のままでよい。**

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

## 3. つまずいた点と教訓

- **`paths` フィルタの罠（#16 で踏んだもの）。** `ci.yml` の `paths` に対象外のディレクトリしか変えていない PR は CI がスキップされ、Deploy Hook も飛ばない（ops-hub では Task 20 以降、`check_ruleset_contract.py` が `paths` / `paths-ignore` を拒否している）
- **`/health` が 503 を返す場合は `DATABASE_URL` が誤っている。** ログの `database = postgres://ops_hub:***@...` を見てホストを確認する

## 4. 再現コマンド

```bash
curl -i https://ops-hub.onrender.com/livez     # 200 ok（DBに触らない）
curl -i https://ops-hub.onrender.com/health    # 200 {"status":"ok",...,"db":"up"}
```

## 5. 次タスクへの引き継ぎ

| 順序 | 内容 | タスク |
|---|---|---|
| 1 | `/livez` を追加して push | 本ドキュメント 1.2（完了） |
| 2 | Blueprint 反映・`DATABASE_URL` 設定・`/livez` と `/health` の疎通確認 | 本ドキュメント 1.3 |
| 3 | `POST /v1/runs` の実装 | Task 06（完了） |
| 4 | GitHub Actions から 202 を確認 | タスク15 |
| 5 | Deploy Hook と Docker ビルドのキャッシュ最適化 | タスク16 |

- 宿題4（Render のリクエストタイムアウト実測、`--max-time 120` の妥当性）はタスク15で確認する。`POST /v1/runs` は 202 を即返す設計（D-1）なので、実質の待ち時間はスピンアップの約1分。120秒は妥当に見えるが、実測で裏を取る
- `render.yaml` が現在のリポジトリに無い。Task 18 の復元範囲（`back_cargo/`）外だったため、復元要否を確認する
