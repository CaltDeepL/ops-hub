# ops-hub

複数の個人プロジェクトを横断して監視し、異常を Slack に通知する**外形監視・通知ハブ**。

無料枠（Render / Neon）の制約を前提に設計している。常駐スケジューラを持たず、
GitHub Actions の定期実行（60分間隔）が `POST /v1/runs` を叩いて1回分のチェックを起動する。
GitHub Actions 側の実行失敗が、そのまま ops-hub 自身のデッドマンスイッチになる。

| 項目 | 内容 |
|---|---|
| スタック | Rust 1.96 / Axum 0.8 / sqlx 0.9 / PostgreSQL 17 / Docker / GitHub Actions |
| 実行基盤 | Render（Web Service・無料枠）+ Neon（PostgreSQL・無料枠） |
| チェック間隔 | 60分 |
| 主要指標 | 応答成功率（成功チェック数 ÷ 総チェック数）。時間ベースの稼働率とは別物 |
| 目標 | 99.0%（月720チェックのうち7回の失敗まで許容） |

> **なぜ「稼働率」と呼ばないか**：無料枠ではスピンダウンが構造的に起きるうえ、
> 監視すること自体がスピンダウンを妨げる（観測者効果）。時間ベースの稼働率を
> 名乗ると定義が破綻するため、指標を応答成功率として定義し直している。

---

## 現在の状態

実装ロードマップ全16タスクのうち：

| # | タスク | 状態 |
|---|---|---|
| 1 | プロジェクト雛形・`/health`・compose・Dockerfile | ✅ 完了 |
| 2 | マイグレーション0001（ENUM・targets・target_states） | 次 |
| 3〜16 | 制約・`AppError`・advisory lock・`POST /v1/runs`・probe・状態遷移・通知・集計・ステータス画面・CI ほか | 未着手 |

各タスクの完了時に `docs/task-NN-<名前>.md` を残している（ゴールと完了条件・設計判断の
根拠・つまずいた点と教訓・次タスクへの引き継ぎ・再現コマンド）。

現在動くエンドポイントは `GET /health` のみ。

---

## ディレクトリ構成

```
.
├── Cargo.toml / Cargo.lock     # Cargo.lock はコミットする（バイナリのため）
├── Dockerfile                  # builder(bookworm) → distroless/cc-debian12
├── compose.yaml                # db(postgres:17) + api
├── .env.example
├── docs/
│   └── task-01-skeleton.md
└── src/
    ├── main.rs                 # CLI（serve / healthcheck）・プール・シャットダウン
    ├── lib.rs                  # ルータ組み立て（統合テストから呼ぶ）
    ├── config.rs               # 環境変数の読み込み・マスキング・-pooler 検出
    ├── state.rs                # AppState
    └── handler/
        ├── mod.rs
        └── health.rs
```

> `src/handler/mod.rs` の位置は変えないこと。Rust は `src/handler.rs` か
> `src/handler/mod.rs` しか見ないため、`src/mod.rs` に置くと `E0583` になる。

---

## 必要なもの

- Rust 1.96 以上（edition 2024 を使う）
- Docker / Docker Compose

```bash
brew install rustup            # Homebrew 版に rustup-init は不要
echo 'export PATH="/opt/homebrew/opt/rustup/bin:$PATH"' >> ~/.zshrc && source ~/.zshrc
rustup default stable
rustc --version                # 1.96.0 以上
```

---

## 起動手順

```bash
cp .env.example .env
cargo generate-lockfile        # 初回のみ（Cargo.lock が無い場合）

docker compose up --build -d
docker compose ps              # api / db がともに healthy になるまで待つ
curl http://localhost:8081/health
# {"status":"ok","version":"0.1.0","db":"up"}
```

ホスト側の公開ポートは既定で **API 8081 / DB 5433**。コンテナ内部は 8080 / 5432 のまま。
変えたい場合は `.env` の `API_HOST_PORT` / `DB_HOST_PORT` を書き換える。

コンテナを使わずに動かす場合：

```bash
docker compose up -d db
set -a && . ./.env && set +a
cargo run                      # サブコマンド省略時は serve
```

---

## 環境変数

| 変数 | 既定 | 用途 |
|---|---|---|
| `DATABASE_URL` | （必須） | PostgreSQL 接続文字列。Neon では **`-pooler` の付かない直接エンドポイント**を指すこと |
| `PORT` | `8080` | 待ち受けポート（コンテナ内部） |
| `DB_MAX_CONNECTIONS` | `5` | プールの最大接続数 |
| `DB_ACQUIRE_TIMEOUT_SECS` | `5` | プールから接続を取得する上限 |
| `RUST_LOG` | `info,sqlx=warn,tower_http=info` | ログレベル |
| `LOG_FORMAT` | `json` | `pretty` で人間向けの整形ログ |
| `API_HOST_PORT` / `DB_HOST_PORT` | `8081` / `5433` | compose がホストに公開するポート（アプリは読まない） |

`DATABASE_URL` はログ出力時にパスワードをマスクする（要件 N-8）。
`-pooler` を含むホストを指している場合、起動時に警告を出す
（セッションレベルの advisory lock がプーラ越しに機能しないため）。

---

## API

| メソッド | パス | 説明 |
|---|---|---|
| GET | `/health` | DB疎通まで確認する。到達不能なら **503** |

```json
// 200
{"status":"ok","version":"0.1.0","db":"up"}
// 503
{"status":"degraded","version":"0.1.0","db":"down"}
```

`POST /v1/runs`・`POST /v1/notifications`・ステータス画面はタスク6以降で追加する。

---

## 開発コマンド

```bash
cargo test                     # ユニットテスト
cargo clippy -- -D warnings
cargo fmt --check

docker compose logs -f api
docker compose exec api /app/ops-hub healthcheck; echo "exit=$?"
docker compose down            # -v を付けるとDBのデータも消える
```

コンテナは distroless で**シェルも curl も入っていない**ため、`docker compose exec api sh`
は使えない。疎通確認はバイナリの `healthcheck` サブコマンドで行う。

---

## トラブルシューティング

| 症状 | 原因と対処 |
|---|---|
| `"/Cargo.lock": not found` | `cargo generate-lockfile` を実行してからビルドする。`Cargo.lock` はコミットする |
| `error[E0583]: file not found for module 'handler'` | `handler/mod.rs` が `src/handler/` 以下に無い |
| `Bind for 0.0.0.0:5432 failed: port is already allocated` | ホストのポートが使用中。`.env` の `DB_HOST_PORT` / `API_HOST_PORT` を変える。`lsof -nP -iTCP:8080 -sTCP:LISTEN` で犯人を特定できる |
| `/health` が 503 | DBに到達できていない。`docker compose ps` で db が healthy か、`DATABASE_URL` のホストとポートが正しいかを確認する |
| ビルドは通るのにコンテナが起動直後に落ちる | builder とランタイムの glibc 版ずれ。builder は bookworm 系に固定すること |