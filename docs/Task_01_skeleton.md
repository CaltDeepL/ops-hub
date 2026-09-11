# タスク01：プロジェクト雛形・/health・compose・Dockerfile

| 項目 | 内容 |
|---|---|
| 上位ドキュメント | ops-hub-detail v1.0 10章 タスク1 |
| 完了条件 | `docker compose up --build -d` でAPIがhealthyになり `/health` が疎通する |
| ステータス | （着手中 / 完了）|

## 1. ゴールと完了条件

- `GET /health` がDB接続状態を含めて返る（N-13）
- `docker compose ps` で api コンテナが `healthy` になる
- DBを止めると `/health` が 503 を返し、コンテナが `unhealthy` に落ちる

## 2. このタスクで作ったもの

| ファイル | 役割 |
|---|---|
| `Cargo.toml` | 依存の確定。sqlx 0.9 / axum 0.8 / edition 2024 |
| `src/main.rs` | clap CLI（`serve` / `healthcheck`）、プール構築、graceful shutdown |
| `src/lib.rs` | `app()` でルータを組み立てる。統合テストからも呼ぶ |
| `src/config.rs` | `DATABASE_URL` / `PORT`、`-pooler` 検出 |
| `src/state.rs` | `AppState { db, config }` |
| `src/handler/health.rs` | `GET /health` |
| `Dockerfile` | rust:1.96-slim-bookworm → distroless/cc-debian12:nonroot |
| `compose.yaml` | postgres:17 + api |

まだ作っていないもの（意図的）：`error.rs`（タスク4）、`migrations/`（タスク2〜3）、`domain/` `service/` `repository/` `provider/`（タスク8以降）。

## 3. 設計判断の根拠

| # | 判断 | 根拠 |
|---|---|---|
| 1 | プールを `connect_lazy` で作る | 起動時にDBへ繋ぎにいかない。スピンダウン前提の環境で、DBの一時的な不調を「起動失敗」に化けさせない。DBの死は `/health` の503で表明する |
| 2 | DB不通時の `/health` は 503 | 200のままだとDockerもプラットフォームも healthy と誤判定する |
| 3 | Docker の HEALTHCHECK に自バイナリの `healthcheck` を使う | distroless にはシェルも curl も無い。設計で clap に `healthcheck` を置いた理由がこれ |
| 4 | SIGTERM を拾って graceful shutdown | スピンダウン時に送られてくる。拾わないと毎回強制終了になる |
| 5 | `-pooler` 検出は警告のみ（起動は止めない） | 詳細設計1.4。ローカルや将来のプール利用を全面禁止にはしない |
| 6 | ビルダを `bookworm` で明示固定 | distroless/cc-debian12 と glibc を揃える。ここがずれると実行時に落ちる |

## 4. つまずいた点と教訓

（実際に動かして埋める）

## 5. 次タスクへの引き継ぎ

- タスク2でマイグレーション0001を作る。`sqlx-cli` は `cargo install sqlx-cli --no-default-features --features rustls,postgres`
- 残る宿題3（Neonの直接エンドポイント確認）はこのタスクの範囲。Neonの接続文字列を `.env` に入れて `serve` を起動し、警告が出ないことを確認する

## 6. 再現コマンド

```bash
cp .env.example .env
docker compose up --build -d
docker compose ps                      # api が healthy になるまで待つ
curl -i http://localhost:8080/health   # 200 / {"status":"ok",...}

docker compose stop db
curl -i http://localhost:8080/health   # 503 / {"status":"degraded",...}
docker compose ps                      # api が unhealthy に落ちる

docker compose down -v
```