# Task 01: プロジェクト雛形・/health・compose・Dockerfile

| 項目 | 内容 |
|---|---|
| 上位ドキュメント | ops-hub-detail v1.0 10章 タスク1 |
| 目的 | API の雛形を作り、DB 接続状態を含む `/health` とコンテナのヘルスチェックを成立させる |
| 完了条件 | `docker compose up --build -d` で API が healthy になり `/health` が疎通する |
| ステータス | 完了（後続タスクがこの雛形の上で動作・検証済み。本タスク単体の検証記録は未記入） |

## 1. 実施内容

### 完了条件の内訳

- `GET /health` が DB 接続状態を含めて返る（N-13）
- `docker compose ps` で api コンテナが `healthy` になる
- DB を止めると `/health` が 503 を返し、コンテナが `unhealthy` に落ちる

### 作成ファイル

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

意図的に作っていないもの：`error.rs`（Task 04）、`migrations/`（Task 02〜03）、`domain/` `service/` `repository/` `provider/`（Task 08 以降）。

## 2. 設計判断

| # | 判断 | 根拠 |
|---|---|---|
| 1 | プールを `connect_lazy` で作る | 起動時に DB へ繋ぎにいかない。スピンダウン前提の環境で、DB の一時的な不調を「起動失敗」に化けさせない。DB の死は `/health` の 503 で表明する |
| 2 | DB 不通時の `/health` は 503 | 200 のままだと Docker もプラットフォームも healthy と誤判定する |
| 3 | Docker の HEALTHCHECK に自バイナリの `healthcheck` を使う | distroless にはシェルも curl も無い。clap に `healthcheck` を置いた理由がこれ |
| 4 | SIGTERM を拾って graceful shutdown | スピンダウン時に送られてくる。拾わないと毎回強制終了になる |
| 5 | `-pooler` 検出は警告のみ（起動は止めない） | 詳細設計 1.4。ローカルや将来のプール利用を全面禁止にはしない |
| 6 | ビルダを `bookworm` で明示固定 | distroless/cc-debian12 と glibc を揃える。ずれると実行時に落ちる |

## 3. つまずいた点と教訓

記録なし（作成当時「実際に動かして埋める」とされたまま未記入）。

## 4. 再現コマンド

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

> 現在の compose ではホスト側 API ポートは `8081`（README 参照）。上記の `8080` は作成当時の値。

## 5. 次タスクへの引き継ぎ

- Task 02 でマイグレーション 0001 を作る。`sqlx-cli` は `cargo install sqlx-cli --no-default-features --features rustls,postgres`
- 宿題3（Neon の直接エンドポイント確認）はこのタスクの範囲。Neon の接続文字列を `.env` に入れて `serve` を起動し、警告が出ないことを確認する（→ Task 05 で完了）
