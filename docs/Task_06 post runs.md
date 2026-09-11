# タスク06 — `POST /v1/runs` の骨格

| 項目 | 内容 |
|---|---|
| 上位ドキュメント | ops-hub-detail v1.0 2.2 / 3.2 / 10章 タスク6 |
| ゴール | advisory lock を入口に、run の開始をDBへ記録し、HTTPは即座に応答する |
| 完了条件 | `runs_test.rs` ① 正常実行 202 + runs作成、② ロック競合 200 + already_running が green |
| 実装範囲外 | 取り残しロック回収、rate limit、実際の probe / incident / outbox 処理 |

> **未実施：`cargo test` を通していない。** この作業環境に Rust ツールチェーンが無く、
> コンパイル検証ができていない。ローカルで6章の手順を回し、5章の想定修正点を
> 潰してからコミットすること。**`cargo sqlx prepare` が本タスクで初めて必要になる**
> （4.2 参照）。

---

## 1. 変更ファイル

### 1.1 新規

- `src/repository/mod.rs` / `src/repository/run_repo.rs`
- `src/service/mod.rs` / `src/service/run_service.rs`
- `src/handler/runs.rs`
- `tests/runs_test.rs`

### 1.2 既存への追記

`src/lib.rs`

```rust
 pub mod config;
 pub mod error;
 pub mod handler;
+pub mod repository;
 pub mod run_lock;
+pub mod service;
 pub mod state;
 pub mod trace_id;

-use axum::{Router, routing::get};
+use axum::{Router, routing::{get, post}};
```

```rust
         .route("/livez", get(handler::livez::livez))
+        .route("/v1/runs", post(handler::runs::start_run))
```

`src/handler/mod.rs`

```rust
 pub mod health;
 pub mod livez;
+pub mod runs;
```

`src/state.rs` — T6-4

```rust
 pub struct AppState {
     pub db: PgPool,
+    pub config: Arc<Config>,
 }
```

`src/main.rs` — **手で当てる必要がある1箇所。**

```rust
 let config = Config::from_env()?;
 let pool = /* connect_lazy(...) */;
-let state = AppState { db: pool };
+let state = AppState { db: pool, config: Arc::new(config) };
```

`config` を move した後も `config.port` を読むなら、`Arc` を先に作って
`state.config.port` を見るか、`port` を先に控えておく。
`use std::sync::Arc;` の追加を忘れないこと。

`Cargo.toml`

```toml
 [dependencies]
+uuid = { version = "1", features = ["serde"] }

+[dev-dependencies]
+tower = { version = "0.5", features = ["util"] }
+http-body-util = "0.1"
```

`serde_json` は既に通常依存なのでテストからも使える。`sqlx` の `uuid`
フィーチャーは有効済みだが、それは「sqlx が `uuid` 型を扱える」という意味で、
アプリ側が `uuid::Uuid` を名前で使うには直接依存が要る。

---

## 2. 実装の対応

| 設計判断 | 実装上の担保 |
|---|---|
| T6-1 空 run も completed に閉じる | `run_repo::complete_empty()`。`execute()` は no-op のあと必ずこれを呼ぶ。テスト`背景処理が終わるとcompletedで締められる`で検証 |
| T6-2 completed 記録 → unlock の順 | `execute()` 内で `complete_empty` → `release(lock)` の順に固定。間で早期 return しないよう `?` を使わず `match` で受ける |
| T6-3 競合時の `run_id = null` を許容 | `latest_running()` が `Option<Uuid>`。`RunOutcome::AlreadyRunning { run_id: Option<Uuid> }` と `RunAcceptedResponse.run_id: Option<Uuid>` まで `Option` のまま通す。500 にしない |
| T6-4 `AppState` に `Config` | `config: Arc<Config>`。リクエストごとに clone されるので `Arc` で包み、`database_url` の `String` を毎回複製しない |

### 2.1 ロック競合は成功型で表す

`RunOutcome::{Started, AlreadyRunning}` を service が返し、ハンドラが 202 / 200 に
写す。`AppError` にバリアントを作らない（詳細設計 3.2、D-3）。409 を返すと
GitHub Actions のステップが失敗し、デッドマンスイッチが誤発報する。
「エラーではない」を型の上で表明しておくと、後からうっかり `AppError` へ
寄せる改変ができなくなる。

### 2.2 締めは `WHERE status = 'running'` を付ける

タスク07のスイーパーが先に `failed` へ倒していた場合に、それを `completed` で
上書きして無かったことにしないため。`complete_empty()` は更新行数を `bool` で
返し、`false`（＝別の主体が既に締めていた）のときは warn を残す。

### 2.3 `run_status` を Rust の enum にしない

この段階では状態を SQL のリテラルとしてしか扱わない。`INSERT INTO runs
DEFAULT VALUES RETURNING id` としているのも同じ理由で、`status` /
`started_at` / `targets_checked` / `notifications_sent` はすべて DEFAULT に
任せている。enum の写像が要るのは `GET /v1/runs` で履歴を返すとき。

### 2.4 記録用のコネクションはプールから別に取る

`RunLock` は `&mut PgConnection` を公開しないので、そもそも間違えようがない。
ロック保持コネクションで長いクエリを流すと、クライアントが消えてもサーバが
切断を検知できず、ロックが残って次回以降の実行が止まる（タスク05 3章の実測）。

### 2.5 INSERT に失敗したらロックを解放してから返す

`insert_running()` が失敗したときだけ `start()` の中で `release` する。
ここを忘れると、記録に失敗しただけでロックが次の実行まで残る。

---

## 3. API契約

| 状況 | ステータス | ボディ |
|---|---|---|
| 正常開始 | `202` | `{"run_id":"<uuid>","status":"started"}` |
| 競合 | `200` | `{"run_id":"<uuid>" または null,"status":"already_running"}` |
| DB接続不可 | `503` | problem+json（`AppError::Database` 経由） |

`recovered: true` はタスク07の回収成功時のみ追加する。通常開始では
**フィールド自体を出さない**ので、`RunAcceptedResponse` には現時点で
`recovered` を持たせていない。

---

## 4. 注意点

### 4.1 `AppState` に `Config` が無かった（T6-4）

タスク05の引き継ぎメモは `state.pool` / `state.config` と書いていたが、実物の
`AppState` は `db: PgPool` の1フィールドだけだった。実物を照合せずに書き
始めていたら、既存プロジェクトのタスク#6と同じ形でコンパイルエラーの山に
なっていた。

### 4.2 `.sqlx` の生成が本タスクで初めて必要になる

`run_repo` で `query!` / `query_scalar!` マクロを使い始めた。Dockerfile には
タスク01の時点で `ENV SQLX_OFFLINE=true` が入っているので、`.sqlx/` が無いまま
`docker compose build` すると**ビルドが落ちる**。5章の手順どおり
`cargo sqlx prepare` を実行し、`.sqlx/` をコミットすること。

### 4.3 いま `POST /v1/runs` は無認証・無制限

rate limit はタスク07へ持ち越した（ロードマップ上の担当タスクが抜けていた）。
それまでの間、公開URLで叩かれても advisory lock が二重実行を止めるので実害は
小さいが、`runs` の行は無制限に増える。タスク14（公開）より前に必ず閉じる。
`AppError::TooManyRequests` はタスク04で用意済みで、まだ誰も使っていない。

---

## 5. つまずきそうな点（未検証なので予測）

| 症状 | 原因と対処 |
|---|---|
| `error[E0583]: file not found for module` | `src/repository/mod.rs` / `src/service/mod.rs` の置き場所。`src/mod.rs` ではない。`pub mod` 宣言忘れは既存プロジェクトで通算5回踏んでいる |
| `error[E0433]: use of undeclared crate uuid` | `Cargo.toml` に `uuid` の直接依存を足していない（1.2） |
| `cargo build` で `.sqlx` 関連のエラー | `cargo sqlx prepare` 未実行。または `DATABASE_URL` がポート5432を向いている（このプロジェクトはホスト側 **5433**） |
| テストが `already_running` で落ちる | 前のテストのロックが残っている。`#[sqlx::test]` は1テスト1DBなので本来起きない。起きたなら `migrations = "./migrations"` の指定漏れで同一DBを共有している |
| `背景処理が終わるとcompletedで締められる` が5秒でタイムアウト | `tokio::spawn` したタスクが走る前にテストが終わっている、または `execute()` の中で panic している。`RUST_LOG=debug cargo test -- --nocapture` でログを見る |
| `PoolTimedOut` | `test_pool` の `max_connections(3)` が効いていない。`#[sqlx::test]` の引数形式（`PoolOptions` + `PgConnectOptions`）はタスク05で動作確認済み |
| let chain（`if let ... && finished`）が通らない | edition 2024 / Rust 1.88 以降の構文。`rust-version = "1.96"` なので通るはずだが、通らなければネストした `if` に戻す |

---

## 6. 再現コマンド

```bash
cd ~/A/ops-hub/back_cargo

# 前提: docker compose up -d db（ホスト側は 5433）
export DATABASE_URL="postgres://ops_hub:ops_hub@localhost:5433/ops_hub"
psql "$DATABASE_URL" -c "select current_database()"   # ops_hub が返ること
sqlx migrate run

# 本タスクで初めて必要になる
cargo sqlx prepare -- --all-targets
git add .sqlx

cargo test --test runs_test -- --nocapture
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check

# 手動確認
docker compose up --build -d
curl -i -X POST http://localhost:8081/v1/runs        # 202 started
psql "$DATABASE_URL" -c "select id, status, started_at, finished_at, targets_checked from runs order by started_at desc limit 5;"

# 競合確認（端末を2つ使う）
# 端末1
psql "$DATABASE_URL" -c "select pg_try_advisory_lock(8421337); select pg_sleep(20);"
# 端末2
curl -i -X POST http://localhost:8081/v1/runs        # 200 already_running
```

`.env` に `SQLX_OFFLINE=true` を書かないこと。既存プロジェクトで、sqlx マクロが
コンパイル時に `.env` を自力で読むために `unset` が効かず詰まった事例がある。

---

## 7. タスク07への引き継ぎ

- `runs_test.rs` ③：取り残しロックを `pg_locks` から見つけ、`pg_terminate_backend`
  後に**1回だけ**再取得する
- `pg_locks` は `classid` / `objid` を結合して bigint key を復元する。
  `objid = $1` だけの旧設計SQLは使わない（タスク05 5.1 の実測）

  ```sql
  WHERE l.locktype = 'advisory'
    AND l.objsubid = 1
    AND ((l.classid::bigint << 32) | l.objid::bigint) = $1
    AND l.granted
    AND l.pid <> pg_backend_pid()
  ```
- 回収成功時は `RunOutcome::Started { recovered: true, .. }`。
  `RunAcceptedResponse` にも `recovered` を足し、通常開始では
  `#[serde(skip_serializing_if)]` でフィールドごと出さない
- 回収できない／権限不足は `AlreadyRunning` へフォールバック
- `runs_test.rs` ④（rate limit）はロードマップ上の担当タスクが抜けているため
  タスク07に含め、endpoint の入口仕様を閉じる
- スイーパーは `running` のまま15分以上経った `runs` を `failed` に倒す。
  `complete_empty()` は `WHERE status = 'running'` を付けてあるので、
  スイーパーと衝突しても上書きは起きない（2.2）
- タスク08で `complete_empty()` は、実際の `targets_checked` /
  `notifications_sent` を受け取る完了処理と、失敗時に `failed` へ倒す処理に
  置き換える。`execute()` の「早期 return しない」構造はそのまま使う

---
