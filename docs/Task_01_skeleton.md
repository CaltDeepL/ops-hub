# タスク01：プロジェクト雛形・`/health`・compose・Dockerfile

| 項目 | 内容 |
|---|---|
| 対応する設計 | ops-hub 詳細設計 v1.0 実装ロードマップ #1 |
| 完了条件 | `docker compose up --build -d` でAPIが healthy になり `/health` が疎通する |
| 状態 | **完了**（macOS / aarch64、rustc 1.96.0、Docker Desktop で確認） |

完了時の応答：

```json
{"status":"ok","version":"0.1.0","db":"up"}
```

API → PostgreSQL の接続まで含めて正常。`docker compose ps` で `api` / `db` とも healthy。

---

## 1. このタスクで作ったもの

| ファイル | 内容 |
|---|---|
| `Cargo.toml` | edition 2024 / rust-version 1.96。axum 0.8・sqlx 0.9・tokio・clap・tower-http |
| `Cargo.lock` | `cargo generate-lockfile` で生成（246パッケージ）。**コミット必須** |
| `src/main.rs` | CLI（`serve` / `healthcheck`）、プール生成、ログ初期化、graceful shutdown |
| `src/lib.rs` | ルータ組み立て。統合テストから `app(state)` を直接呼べるようにライブラリ側に置く |
| `src/config.rs` | 環境変数からの設定読み込み、認証情報のマスキング、`-pooler` 検出。ユニットテスト6件 |
| `src/state.rs` | `AppState`（`PgPool` のみ） |
| `src/handler/mod.rs` | `pub mod health;` |
| `src/handler/health.rs` | `GET /health`。DB疎通まで見て、駄目なら503 |
| `Dockerfile` | 2段ビルド（`rust:1.96-slim-bookworm` → `distroless/cc-debian12:nonroot`） |
| `compose.yaml` | `db`（postgres:17）+ `api`。`service_healthy` で起動順を制御 |
| `README.md` / `.env.example` / `.gitignore` / `.dockerignore` | — |

この段階で**意図的に無いもの**：`error.rs`（タスク4）、`migrations/`（タスク2）、
`domain/` `service/` `repository/` `provider/`（タスク8以降）。
`/health` はDBの疎通しか見ないので、まだ `AppError` を必要としない。

---

## 2. 設計判断の根拠

| # | 判断 | 根拠 |
|---|---|---|
| T1-1 | プールは `connect_lazy` で作る | プロセスはスピンダウンから何度も起き直す。起動時にDBへ繋ぎにいく設計だと、Neonが一時的に応答しないだけでプロセスが上がらない。DBの状態は起動可否ではなく `/health` の応答で表現する |
| T1-2 | `/health` はDB到達不能時に **503** を返す | 200固定だと、外形監視から見て「ops-hub は生きているがDBが死んでいる」状態が透明になる。自分自身が監視される側（デッドマンスイッチの対象）なので、ここは正直に落とす |
| T1-3 | DB疎通に3秒の独自タイムアウトを張る | プールの `acquire_timeout` は接続取得までしか効かない。接続後にサーバが黙るケースで `/health` が固まると、コンテナ側の HEALTHCHECK タイムアウトとして観測され原因が分かりにくくなる |
| T1-4 | HEALTHCHECK は自前バイナリの `healthcheck` サブコマンド | distroless にはシェルも curl も無い。イメージにcurlを足すと攻撃面が増えるので、バイナリ自身に持たせる。exec 形式で書く必要がある |
| T1-5 | 設定はCLIフラグを受け付けず環境変数のみ | Render も compose も設定は環境変数で渡す。入口を1本にしておくと「フラグと環境変数のどちらが効いているか」を障害時に考えなくて済む |
| T1-6 | 接続文字列はマスクしてからログに出す（N-8） | パスワードに `@` が入りうるので、最後の `@` で切る実装にした。ユニットテストで固定 |
| T1-7 | 起動時に `-pooler` を検出して警告 | セッションレベルの advisory lock はトランザクションプーラ越しでは機能せず、**エラーにならず静かに排他が壊れる**。タスク5の前提を起動時に守る |
| T1-8 | builder のベースを bookworm に固定 | ランタイムの `distroless/cc-debian12` と glibc 版を揃える。ずれると起動直後に落ちる形で出る |
| T1-9 | ログは既定でJSON（N-12）、`LOG_FORMAT=pretty` で切替 | 本番は構造化、ローカルは可読性 |
| T1-10 | ルータを `lib.rs` に置く | タスク6以降の `tests/runs_test.rs` などから、サーバを起動せずに `app(state)` を組み立てられる |
| T1-11 | ホスト側の公開ポートを `.env` で可変にする（既定 8081 / 5433） | 開発機では 8080・5432 が他プロセスに埋まっていることが多い（5章 S-3）。**コンテナ内部は 8080 / 5432 のまま**なので、Dockerfile・アプリ・本番構成には影響しない |

---

## 3. `/health` の仕様

| 条件 | ステータス | ボディ |
|---|---|---|
| `SELECT 1` が成功 | 200 | `{"status":"ok","version":"0.1.0","db":"up"}` |
| 失敗・3秒でタイムアウト | 503 | `{"status":"degraded","version":"0.1.0","db":"down"}` |

失敗理由は `warn` でログに出すだけで、レスポンスには載せない（内部情報を出さない）。

---

## 4. 詳細設計11章「残る宿題」#3 への対応

> Neonの直接エンドポイント（`-pooler` なし）で接続していることを確認する（タスク1）

コード側の対応（T1-7）は入れたが、**Neonへの実接続での確認は未実施**（ローカルの
compose では該当しないため）。Render に環境変数を設定する時点で、起動ログに
`-pooler` の警告が出ていないことを確認する。→ タスク15まで持ち越し。

---

## 5. つまずいた点と教訓

| # | 事象 | 原因 | 対処 |
|---|---|---|---|
| S-1 | `"/Cargo.lock": not found` で `docker build` が失敗 | Dockerfile が依存キャッシュのために `COPY Cargo.toml Cargo.lock ./` をしているが、`Cargo.lock` を作る前にビルドした | `cargo generate-lockfile`（246パッケージ）。**バイナリなので `Cargo.lock` はコミットする**。ローカルで一度 `cargo build` してからDockerに渡す順序を守る |
| S-2 | `error[E0583]: file not found for module 'handler'` | `handler/mod.rs` を `src/mod.rs` として置いてしまっていた。Rust のモジュール解決は `src/handler.rs` か `src/handler/mod.rs` のどちらかしか見ない | `src/handler/mod.rs` へ移動。ファイル名だけでなく**どのディレクトリに置くか**がモジュール宣言と結びついている |
| S-3 | `Bind for 0.0.0.0:5432 failed: port is already allocated`、続いて 8080 でも同じ | ホスト側の 5432（別のPostgreSQL）と 8080（別プロセス）が使用中 | ホスト公開ポートのみ 5433 / 8081 に変更。コンテナ内部は変えない。以降は `.env` の `DB_HOST_PORT` / `API_HOST_PORT` で切り替える（T1-11） |

**教訓**：S-3 は「コンテナ内部のポート」と「ホストに公開するポート」を分けて考えれば
アプリ側の設定を一切触らずに済む。ここで `PORT` を 8081 に変えてしまうと、
Dockerfile の `EXPOSE` と HEALTHCHECK、本番の Render 設定まで芋づるで狂う。

現在のポート構成：

```
Mac
 ├─ localhost:8081 ──> ops-hub-api :8080 ──> ops-hub-db :5432
 └─ localhost:5433 ──────────────────────>  ops-hub-db :5432
```

---

## 6. 再現コマンド

```bash
# 0. 準備
cp .env.example .env        # ポートが埋まっていれば API_HOST_PORT / DB_HOST_PORT を変える
cargo generate-lockfile     # 初回のみ（Cargo.lock が無い場合）

# 1. ローカルでコンパイルとテスト
cargo build
cargo test                  # config.rs のユニットテスト6件
cargo clippy -- -D warnings
cargo fmt --check

# 2. 完了条件の確認
docker compose up --build -d
docker compose ps                       # api / db がともに healthy（最大30秒程度）
curl -i http://localhost:8081/health    # 200 {"status":"ok",...,"db":"up"}
docker compose logs api                 # JSON1行ログ・接続文字列がマスクされていること

# 3. DBを落として 503 を確認する（T1-2）
docker compose stop db
curl -i http://localhost:8081/health    # 503 {"status":"degraded",...,"db":"down"}
docker compose start db

# 4. HEALTHCHECK サブコマンド単体（コンテナ内部なので 8080）
docker compose exec api /app/ops-hub healthcheck; echo "exit=$?"

# 5. ポート競合を調べる
lsof -nP -iTCP:8080 -sTCP:LISTEN
docker ps --format "table {{.Names}}\t{{.Ports}}"
```

---

## 7. 次タスクへの引き継ぎ

- **タスク2（マイグレーション0001）**
  ```bash
  cargo install sqlx-cli --no-default-features --features rustls,postgres
  export DATABASE_URL=postgres://ops_hub:ops_hub@localhost:5433/ops_hub   # ポート注意
  ```
  ファイル名は sqlx の `{version}_{description}.{up|down}.sql` に合わせ、
  基本設計2.2の命名で `migrations/0001_init.up.sql` を手で置く（`sqlx migrate add -r`
  だとタイムスタンプ版になる）。
- `Dockerfile` に `SQLX_OFFLINE=true` を先に入れてある。タスク4以降で `query!` マクロを
  使い始めたら `cargo sqlx prepare` で `.sqlx/` を生成してコミットする。
- `migrations/` は `.dockerignore` に含めていない（実行時にマイグレーションを
  適用する方式へ切り替える可能性を残すため）。
- `error.rs` はタスク4で追加する。それまで `/health` は `AppError` を使わない。
- 宿題#3（Neon 直接エンドポイントの確認）はタスク15へ持ち越し（4章）。