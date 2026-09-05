# タスク05 — `RunLock`（advisory lock）

## 1. ゴールと完了条件

| 項目 | 内容 |
|---|---|
| ゴール | `POST /v1/runs` の排他制御を担う `RunLock` を実装する（詳細設計 1.1〜1.2） |
| 完了条件 | 同一DB内の2コネクションで競合が再現し、`release` 後に再取得できる |
| 実装範囲 | `src/run_lock.rs` / `tests/run_lock_test.rs` / `Config::run_lock_key` |
| 実装範囲外 | 取り残しロックの脱出ハッチ（タスク7）、`runs` テーブルへの記録（タスク6） |
| 状態 | **完了。** `cargo test` / `cargo clippy -D warnings` ともに緑 |

```
running 4 tests
test キーが違えば競合しない ... ok
test 保持中は別コネクションから取得できない ... ok
test release後に再取得できる ... ok
test releaseせずにdropしても解放される ... ok
test result: ok. 4 passed; 0 failed
```

このタスクの途中で Neon と Render の作成も行い、詳細設計11章の宿題1〜3が閉じた（6章）。
インフラの手順そのものは `docs/neon_setup.md` / `docs/render_setup.md` に分けた。

---

## 2. 変更ファイル

### 2.1 新規

- `src/run_lock.rs`
- `tests/run_lock_test.rs`

### 2.2 既存への追記

`src/lib.rs`

```rust
 pub mod config;
 pub mod error;
 pub mod handler;
+pub mod run_lock;
 pub mod state;
 pub mod trace_id;
```

`src/config.rs`

```rust
 pub struct Config {
     pub database_url: String,
     pub port: u16,
     pub db_max_connections: u32,
     pub db_acquire_timeout: Duration,
+    /// advisory lock のキー。DB単位のスコープなので環境ごとに変える必要はない。
+    pub run_lock_key: i64,
 }
```

```rust
+        let run_lock_key = parse_env("RUN_LOCK_KEY", 8_421_337_i64)?;
+        // 負の値を許すと pg_locks からキーを復元する際に符号拡張が必要になり、
+        // 脱出ハッチ（タスク7）のSQLが静かに壊れる。入口で弾く。
+        if run_lock_key <= 0 {
+            return Err(anyhow!("環境変数 RUN_LOCK_KEY は正の整数にしてください"));
+        }
```

`Cargo.toml` — `#[sqlx::test]` に `migrate` フィーチャが必要。

```toml
 sqlx = { version = "0.9", default-features = false, features = [
   ...
   "macros",
+  "migrate",
 ] }
```

`.env.example` / README の環境変数表

```
# advisory lock のキー。正の整数。通常は変更しない
RUN_LOCK_KEY=8421337
```

---

## 3. 実測で確認したこと

PostgreSQL 16.15（ローカル・TCP接続）で検証し、Neon の PostgreSQL 17.11 でも
`scripts/verify-neon.sh` により A〜D 相当を再確認した。

| # | 検証 | 結果 |
|---|---|---|
| A | 保持中に別コネクションが `pg_try_advisory_lock` → `false`。`unlock` 後は `true` | 期待どおり（完了条件） |
| B | キーが32bitを超えると `pg_locks` は `classid`(上位32bit) / `objid`(下位32bit) に**分割**して持つ | **要注意。5.1** |
| C | 同一セッションで2回取得すると両方 `true`。解放にも2回の `unlock` が要る | **要注意。5.2** |
| D | セッションが切れるとロックは自動解放される | 期待どおり |
| E | Neon 上でも `pg_terminate_backend` で他セッションを落とせる | 宿題2が閉じた |

### 検証Bの詳細

```
key = 9223372036854775807 でロック
 classid    |   objid    | objsubid | granted
------------+------------+----------+---------
 2147483647 | 4294967295 |        1 | t
```

`(classid::bigint << 32) | objid::bigint` で元のキーに戻る（正のキーに限る）。
既定キー `8421337` は32bitに収まるため `classid = 0 / objid = 8421337 / objsubid = 1` になる。

---

## 4. 設計判断の根拠

### D-a: `PoolConnection` を `RunLock` に move し、外へ出さない

詳細設計 1.2 の制約をコメントではなく型で守る。`&mut PgConnection` を返すメソッドを
一切生やしていないので、「ロック保持コネクションで他のクエリを流す」ミスは
コンパイル時に不可能になる。

### D-b: `release()` は `self` を消費する

二重解放がコンパイルエラーになる。検証Cのとおり advisory lock はカウンタなので、
取得1回・解放2回だと2回目が `false` を返す（＝バグの兆候が実行時までわからない）。

### D-c: `Drop` ではプールに返さず `detach()` して切断する

**詳細設計 1.2 の記述はここだけ誤っていた。** 設計書は「パニック時はコネクションが
プールへ返却される際に破棄され、ロックはセッション終了とともに解放される」と書いているが、
sqlx の `PoolConnection` を drop すると**セッションは生きたままアイドル接続としてプールへ
戻る**。advisory lock はセッションスコープなので解放されない。

さらに、そのコネクションが次の `try_acquire` で再利用されると、検証Cのとおり同一セッション
からの取得は必ず `true` になる。**`try_acquire` が「取れた」と嘘をつき、二重実行が起きる。**
例外もエラーも出ない壊れ方なので、`Drop` で `detach()` してソケットごと閉じる方針にした。

代案の `tokio::spawn` で非同期に `pg_advisory_unlock` を投げる方式は、
ランタイム停止中の drop で `spawn` がパニックすること、解放完了を誰も待てずタイミング依存が
残ることから採らなかった。切断なら「ソケットが閉じた＝解放が確定」で推論が単純になる。

`tests/run_lock_test.rs` の `releaseせずにdropしても解放される` がこの経路を守っている。

### D-d: 取得失敗を `AppError` にしない（詳細設計 3.2 の再確認）

`Ok(None)` で返す。競合は正常系であり、409 を返すと GitHub Actions のワークフローが
失敗してデッドマンスイッチが誤発報する（D-3）。

### D-e: テストのプールを `max_connections(2)` で作り直す

既定のままだと、失敗時に「競合が再現しなかった」のか「プールが枯渇して acquire が
タイムアウトした」のかが区別できない。完了条件が「同一DB内2コネクション」なので、その2を明示する。

---

## 5. つまずいた点と教訓

### 5.1 `pg_locks` のキーは32bitずつに割れている

検証B。詳細設計 1.3 の脱出ハッチSQLは

```sql
WHERE l.locktype = 'advisory' AND l.objid = $1
```

と書いてあるが、これは**キーが32bitに収まる場合しか合っていない**。既定値 `8421337` なら
偶然動く（`classid = 0`）。`RUN_LOCK_KEY` を大きな値に変えた瞬間に「ロックはあるのに
見つからない」→ 脱出ハッチが発動しない、という壊れ方をする。

タスク7では次の述語を使う。`objsubid = 1` は「単一bigint形式のadvisory lock」の印。

```sql
WHERE l.locktype = 'advisory'
  AND l.objsubid = 1
  AND ((l.classid::bigint << 32) | l.objid::bigint) = $1
  AND l.granted
  AND l.pid <> pg_backend_pid()
```

`RUN_LOCK_KEY` を正の値に制限したのはこの述語のため（負だと符号拡張が要る）。
この述語は `scripts/verify-neon.sh` に組み込んであり、Neon 上で動作を確認済み。

### 5.2 同一セッションからの取得は必ず成功する

検証C。`try_acquire` は「他のプロセスが実行中か」を判定しているつもりでも、同じセッションを
再利用した場合は判定になっていない。D-c の `Drop` 実装がこの前提を守っている。
**`Drop for RunLock` を消したり `detach()` を「プールへ返す」に変えたりしないこと。**

### 5.3 コンパイル前に不安だった点（すべて杞憂だった）

記録として残す。sqlx 0.9 で以下はいずれもそのまま通った。

- `PoolConnection::detach(self) -> DB::Connection` はシグネチャが変わっていない
- `#[sqlx::test]` は `(PoolOptions<Postgres>, PgConnectOptions)` を引数に取る形式を受け付ける
- テスト関数名に日本語を使っても `non_snake_case` は出ない（`allow` 不要）
- `sqlx::query_scalar` の `bool` は型注釈なしで推論される

### 5.4 テストは Neon ではなくローカルの Postgres に向ける

`#[sqlx::test]` はテストごとにデータベースを作って捨てる。Neon に向けるとストレージと
compute 時間を無駄に食うので、`DATABASE_URL` は `docker compose` の Postgres
（`localhost:5433`）を指すこと。

---

## 6. 宿題の消化（詳細設計11章）

### 宿題1: `tcp_keepalives_idle` → **閉じた（対応不要）**

まず、前回「判定不能」だった原因が判明した。**Unixソケット接続では
`SHOW tcp_keepalives_idle` が常に 0 を返す。** 環境の問題ではなかった。

Neon（TCP）で実測した結果：

```
$ psql "$NEON_DIRECT_URL" -c "show tcp_keepalives_idle;"
 60

$ psql "$NEON_DIRECT_URL" -c "set tcp_keepalives_idle = 30; show tcp_keepalives_idle;"
SET
 30
```

**Neon の既定は 60 秒**（素の PostgreSQL の 7200 秒ではなく、Neon 側が短く設定している）。
60秒間隔で keepalive が飛ぶなら、ロック保持コネクションが NAT やロードバランサに
切られる心配は実質ない。`SET` も通るので必要になれば後から手当てできるが、
**現時点ではアプリ側の対応は不要**と判断する。

### 宿題2: `pg_terminate_backend` の可否 → **閉じた（可能）**

Neon 上で、同一ロールの別セッションを強制終了できることを確認した。
切断後にロックが自動解放されることも確認済み。
**タスク7の脱出ハッチは「実装できる」前提で設計に入れてよい。**

### 宿題3: 直接エンドポイントの確認 → **閉じた**

`ep-young-sound-b3t3at5u.c-4.ap-southeast-1.aws.neon.tech`（`-pooler` なし）。
Neon の pooler は PgBouncer の transaction モードで、**セッションレベルの advisory lock は
公式に非対応**。掴むと `try_acquire` が常に `true` を返し、例外もエラーも出ないまま
二重実行が止まらなくなる。`scripts/verify-neon.sh` は `-pooler` を検知したらそこで中止する。

---

## 7. 次タスクへの引き継ぎ（タスク6）

- タスク6は `POST /v1/runs` の骨格（ロック取得・`runs` 記録・202応答）
- ハンドラは `RunLock::try_acquire(&state.pool, state.config.run_lock_key)` を呼び、
  `None` なら **200 + `already_running`**（409 にしない）
- `runs` への INSERT は**ロック保持コネクションではなく**プールから別に取ること。
  `RunLock` は `&mut PgConnection` を出さないので、間違えようがない作りにはなっている
- 202 を返してからバックグラウンドで処理する（D-1）。`tokio::spawn` したタスクへ
  `RunLock` を move し、処理の最後で `release().await` する。spawn 前に `release` しない
- `runs_test.rs` の①②がタスク6の完了条件

---

## 8. 再現コマンド

```bash
cd back_cargo
export DATABASE_URL="postgres://ops_hub:ops_hub@localhost:5433/ops_hub"
psql "$DATABASE_URL" -c "select current_database()"   # ops_hub が返ること

cargo test --test run_lock_test -- --nocapture
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

SQLレベルで手早く確かめたいとき（3章の検証A）:

```bash
# 端末1: ロックを保持したまま10秒待つ
psql "$DATABASE_URL" -c "select pg_try_advisory_lock(8421337); select pg_sleep(10);"

# 端末2: 保持中は f、端末1が終われば t
psql "$DATABASE_URL" -At -c "select pg_try_advisory_lock(8421337);"
```

Neon 側の前提を確認するとき:

```bash
set -a && source .env && set +a
DATABASE_URL="$NEON_DIRECT_URL" ./scripts/verify-neon.sh
```