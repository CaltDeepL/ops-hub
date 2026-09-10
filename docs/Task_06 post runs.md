# タスク06 — `POST /v1/runs`（起動・排他・記録）

## 1. ゴールと完了条件

| 項目 | 内容 |
|---|---|
| ゴール | 実行の入口を作る。ロックを取り、`runs` に記録し、202 を返して背後で締める |
| 完了条件 | ① 起動時に 202 と `run_id` が返り `runs` に行ができる ② 競合時に 200 + `already_running` が返る |
| 実装範囲 | `handler/runs.rs` / `service/run_service.rs` / `repository/run_repo.rs` / `AppState` への `config` 追加 / `tests/runs_test.rs` |
| 実装範囲外 | 巡回の中身（タスク8）、取り残しロックの回収と `recovered: true`（タスク7）、スイーパー（タスク7）、レート制限429（後述4.3）、`GET /v1/runs`（タスク12想定） |

> **未実施：`cargo test` を通していない。** この作業環境に Rust ツールチェーンが無く、
> コンパイル検証ができていない。ローカルで 6章の手順を回し、5章の想定修正点を
> 潰してからコミットすること。**`cargo sqlx prepare` が本タスクで初めて必要になる**
> （4.4 参照）。

---

## 2. 変更ファイル

### 2.1 新規

- `src/repository/mod.rs` / `src/repository/run_repo.rs`
- `src/service/mod.rs` / `src/service/run_service.rs`
- `src/handler/runs.rs`
- `tests/runs_test.rs`

### 2.2 既存への追記

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

`src/state.rs` — `Config` を持たせる（4.1）

```rust
 pub struct AppState {
     pub db: PgPool,
+    pub config: Arc<Config>,
 }
```

`src/main.rs` — **手で当てる必要がある1箇所。** `AppState` の組み立てを直す。

```rust
 let config = Config::from_env()?;
 let pool = /* connect_lazy(...) */;
-let state = AppState { db: pool };
+let state = AppState { db: pool, config: Arc::new(config) };
```

`config` を `AppState` に move した後も `config.port` を使うなら、`Arc` を先に
作って `state.config.port` を読むか、`port` を先に控えておく。
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

## 3. 設計判断

### 3.1 ロック競合は成功型で表す

`RunOutcome::{Started, AlreadyRunning}` を service が返し、ハンドラが 202 / 200 に
写す。`AppError` にバリアントを作らない（詳細設計 3.2、D-3）。409 を返すと
GitHub Actions のステップが失敗し、**デッドマンスイッチが誤発報する**。
「エラーではない」を型の上で表明しておくと、後からうっかり `AppError` に
寄せる改変ができなくなる。

### 3.2 背景処理は空箱を1つだけ用意する

`run_service::perform()` が唯一の空箱で、いまは `Ok(0)` を返すだけ。
タスク8はこの関数の中身を書くことになる。`execute()`（締めと解放）を先に
確定させておくことで、タスク8で「失敗時に `runs` が `running` のまま残る」
「ロックが解放されない」という失敗の入り口を作らずに済む。

`execute()` の中では `?` を使わない。早期 return すると締めと解放を飛ばすため。
失敗しうる処理は `perform()` に閉じ込め、`Result` を値として受けている。

### 3.3 締めは `WHERE status = 'running'` を付ける

タスク7のスイーパーが先に `failed` へ倒していた場合に、それを `completed` で
上書きして無かったことにしないため。更新行数が 0 のときは warn を残す
（10分以上掛かった証拠）。

### 3.4 `run_status` を Rust の enum にしない

この段階では状態を SQL のリテラルとしてしか扱わない。`#[derive(sqlx::Type)]` と
`SELECT status as "status: RunStatus"` のキャスト注釈を持ち込む価値が出るのは
`GET /v1/runs` で履歴を返すときなので、そこまで遅らせる。
`INSERT INTO runs DEFAULT VALUES RETURNING id` にしているのも同じ理由で、
`status` / `started_at` はどちらも DEFAULT に任せている。

### 3.5 記録用のコネクションはプールから別に取る

`RunLock` は `&mut PgConnection` を公開しないので、そもそも間違えようがない。
ロック保持コネクションで長いクエリを流すと、クライアントが消えてもサーバが
切断を検知できず、ロックが残って次回以降の実行が止まる（タスク5 3章の実測）。

---

## 4. 判断が要る／持ち越した点

### 4.1 `AppState` に `Config` が無かった

タスク5の引き継ぎメモは `state.pool` / `state.config` と書いていたが、実際の
`AppState` は `db: PgPool` の1フィールドだけだった。`run_lock_key` を読むために
`config: Arc<Config>` を追加している。**引き継ぎメモの記述を実物と照合せずに
書き始めていたら、ここでコンパイルエラーの山になっていた**（既存プロジェクトの
タスク#6で57件出した件と同じ形）。

`Config` を直に持たず `Arc` にしたのは、`AppState` がリクエストごとに clone
されるため。`database_url` の `String` を毎回複製する必要はない。

### 4.2 `recovered: true` は未実装

取り残しロックを回収して起動した場合の応答（詳細設計 2.2）。回収そのものが
タスク7の脱出ハッチなので、フィールドごと出していない。タスク7で
`RunOutcome::Started { run_id, recovered: bool }` に広げる。

### 4.3 レート制限（429）は未実装

基本設計 2.2 に「同一IPから1分1回」とあるが、骨格の範囲外として持ち越した。
`runs.started_at` の最新値を見る簡易方式なので実装自体は小さい。
**ただしこれが無い間、`POST /v1/runs` は誰でも叩ける。** 認証も無い。
公開URLで叩かれても advisory lock が二重実行を止めるので実害は小さいが、
`runs` の行は無制限に増える。タスク7かタスク14（CI・公開）までに入れること。

### 4.4 `.sqlx` の生成が本タスクで初めて必要になる

`run_repo` で `query!` / `query_scalar!` マクロを使い始めた。Dockerfile には
タスク1の時点で `ENV SQLX_OFFLINE=true` が入っているので、`.sqlx/` が無いまま
`docker compose build` すると**ビルドが落ちる**。6章の手順どおり
`cargo sqlx prepare` を実行し、`.sqlx/` をコミットすること。

---

## 5. つまずきそうな点（未検証なので予測）

| 症状 | 原因と対処 |
|---|---|
| `error[E0583]: file not found for module` | `src/repository/mod.rs` / `src/service/mod.rs` の置き場所。`src/mod.rs` ではない。`pub mod` 宣言忘れは既存プロジェクトで通算5回踏んでいる |
| `error[E0433]: use of undeclared crate uuid` | `Cargo.toml` に `uuid` の直接依存を足していない（2.2） |
| `cargo build` で `.sqlx` 関連のエラー | `cargo sqlx prepare` 未実行。または `DATABASE_URL` がポート5432を向いている（このプロジェクトはホスト側 **5433**） |
| テストが `already_running` で落ちる | 前のテストのロックが残っている。`#[sqlx::test]` は1テスト1DBなので本来起きない。起きたなら `migrations = "./migrations"` の指定漏れで同一DBを共有している |
| `背景処理が終わるとcompletedで締められる` が5秒でタイムアウト | `tokio::spawn` したタスクが走る前にテストが終わっている、または `execute()` の中で panic している。`RUST_LOG=debug cargo test -- --nocapture` でログを見る |
| `PoolTimedOut` | `test_pool` の `max_connections(3)` が効いていない。`#[sqlx::test]` の引数形式（`PoolOptions` + `PgConnectOptions`）はタスク5で動作確認済み |
| let chain（`if let ... && finished`）が通らない | edition 2024 / Rust 1.88 以降の構文。`rust-version = "1.96"` なので通るはずだが、通らなければネストした `if` に戻す |

---

## 6. 再現コマンド

```bash
cd ~/A/ops-hub/back_cargo

# 前提: docker compose up -d db（ホスト側は 5433）
export DATABASE_URL="postgres://ops_hub:ops_hub@localhost:5433/ops_hub"
psql "$DATABASE_URL" -c "select current_database()"   # ops_hub が返ること

# 0. マイグレーション適用済みであること
sqlx migrate run

# 1. オフラインクエリキャッシュの生成（本タスクで初めて必要）
cargo sqlx prepare -- --all-targets
git add .sqlx

# 2. ビルドとテスト
cargo test --test runs_test -- --nocapture
cargo test                       # 既存のユニットテストも含めて
cargo clippy --all-targets -- -D warnings
cargo fmt --check

# 3. 手で叩いて確認する
docker compose up --build -d
curl -i -X POST http://localhost:8081/v1/runs     # 202 started
psql "$DATABASE_URL" -c "select id, status, started_at, finished_at, targets_checked from runs order by started_at desc limit 5;"

# 4. 競合を再現する（端末を2つ使う）
# 端末1: ロックを握って10秒待つ
psql "$DATABASE_URL" -c "select pg_try_advisory_lock(8421337); select pg_sleep(10);"
# 端末2: その間に叩く → 200 already_running
curl -i -X POST http://localhost:8081/v1/runs
```

`.env` に `SQLX_OFFLINE=true` を書かないこと。既存プロジェクトで、sqlx マクロが
コンパイル時に `.env` を自力で読むために `unset` が効かず詰まった事例がある。

---

## 7. 次タスクへの引き継ぎ（タスク7）

- タスク7は取り残しロックの脱出ハッチとスイーパー
- `pg_locks` の述語は `objid` 単体で比較してはいけない（タスク5 5.1）

  ```sql
  WHERE l.locktype = 'advisory'
    AND l.objsubid = 1
    AND ((l.classid::bigint << 32) | l.objid::bigint) = $1
    AND l.granted
    AND l.pid <> pg_backend_pid()
  ```
- スイーパーは `running` のまま10分以上経った `runs` を `failed` に倒す。
  `run_repo::finish_completed` / `finish_failed` は `WHERE status = 'running'`
  を付けてあるので、スイーパーと衝突しても上書きは起きない（3.3）
- 回収して起動した場合は `RunOutcome::Started` に `recovered: bool` を足し、
  `RunAcceptedResponse` にも同名のフィールドを追加する（4.2）
- レート制限（429）を入れるならここ（4.3）。`AppError::TooManyRequests` は
  タスク4で用意済みで、まだ誰も使っていない

---
