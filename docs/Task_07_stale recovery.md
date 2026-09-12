# Task 07: 取り残しの回収（スイーパー + 脱出ハッチ）

## 1. 背景

巡回処理には2種類の取り残しがあり、既存コードにも Task 07 の対応対象として残されていた。

| 対象 | 原因 | 排他への影響 | 回収 |
|---|---|---|---|
| `running` のまま残った `runs` | 実行中にプロセス終了 | なし | 自動 |
| advisory lock | セッションが生存したまま停止 | あり | 手動 |

通常発生しうる stale run は自動回収し、セッション切断を伴う advisory lock の解除は誤操作防止のため手動とした。

---

## 2. 実施したこと

### `src/recovery.rs`

以下を追加した。

- `sweep_stale_runs`
  - 閾値を超えて `running` のまま残った run を `failed` に更新
- `lock_key_parts`
  - 64bit advisory lock key を `pg_locks` の `classid` / `objid` に分解
- `lock_holder`
  - lock 保持セッションの PID・state・idle時間・接続元を取得
- `terminate_holder`
  - `pg_terminate_backend` で保持セッションを切断

### `src/config.rs`

以下を追加した。

```text
RUN_STALE_AFTER_SECS
````

既定値は600秒。

60秒未満または非有限値は拒否する。

### `src/service/run_service.rs`

`start()` の lock 取得前に stale run sweep を実行する。

```text
start
  ↓
sweep stale runs
  ↓
advisory lock
  ↓
run
```

sweep に失敗しても巡回自体は継続し、warn log のみ出す。

### `src/main.rs`

`unlock` サブコマンドを追加した。

```bash
# 状態確認のみ
ops-hub unlock

# lock holder を切断
ops-hub unlock --force

# 最小 idle 時間を指定
ops-hub unlock --force --min-idle-secs 120
```

`--force` 指定時も、idle 時間が閾値未満の場合は切断しない。

---

## 3. 設計上の判断

### stale run は自動回収

閾値を超えた `running` は `failed` に倒す。

後から本来の処理が終了しても、既存の finish query が、

```sql
WHERE status = 'running'
```

を持つため `failed` を上書きしない。

### advisory lock は手動解除

lock holder の切断は正常実行中のセッションを殺す可能性があるため、自動化しない。

```text
inspect
  ↓
idle 判定
  ↓
--force
  ↓
terminate
```

という手順を要求する。

### `pg_locks` は runtime query

PostgreSQL system catalog の version 差異を `.sqlx` metadata に固定しないため、`lock_holder` は `sqlx::query` を使用する。

---

## 4. 実装中に発生した問題

### `f64` の型推論

config test の、

```rust
[60.0, 600.0, 3600.0]
```

で `is_finite()` の型を決定できなかった。

`f64` を明示して修正した。

### `Config` の既存初期化

`run_stale_after_secs` 追加により既存 test の `Config` initializer が不足したため、600秒を追加した。

### test 名の warning

日本語 test 名中の `None` / `PID` が `non_snake_case` warning になり、`clippy -D warnings` に備えて修正した。

### rustfmt

最初の `make verify` は `cargo fmt --check` で停止した。

```bash
cargo fmt --all
```

を適用後、再実行して解消した。

---

## 5. SQLx metadata

`sweep_stale_runs` で新しい compile-time query を追加したため `.sqlx` を再生成した。

```bash
make sqlx-prepare
```

結果:

```text
query data written to .sqlx
```

その後 `make verify` 内の、

```text
cargo sqlx prepare --check
```

も PASS した。

---

## 6. 検証結果

ローカル PostgreSQL を起動して migration 適用後、以下を実行した。

```bash
DATABASE_URL=postgres://ops_hub:ops_hub@localhost:5433/ops_hub make verify
```

結果:

```text
DATABASE_URL guard        PASS
Ruleset contract          PASS
cargo fmt --check         PASS
cargo clippy -D warnings  PASS
cargo sqlx prepare --check PASS
cargo test                PASS
```

テスト結果:

```text
unit tests       29 passed
recovery tests   11 passed
run lock tests    4 passed
runs tests        5 passed

total            49 passed
failed            0
```

recovery test では以下を確認した。

* stale run を `failed` に更新
* 閾値内の run は変更しない
* 完了済み run は対象外
* 複数 run の一括回収
* sweep 後の finish が空振りする
* lock 未保持時は `None`
* lock holder を取得できる
* 上位bitを含む lock key を扱える
* session terminate により lock が解放される
* 存在しない PID の terminate が `false`

Task 07 完了。

---

## 7. 次タスクへの引き継ぎ

次は Task 08。

```text
run_service::perform
```

の実処理を実装する。

対象:

```text
targets の取得
    ↓
probe
    ↓
checks の記録
```

現在の `perform` は `Ok(0)` を返す空実装。

また本番環境で `unlock` を実行する運用経路は未確定。

Render Shell が利用できない場合は、管理用 endpoint または GitHub Actions `workflow_dispatch` 等を別途検討する。

````