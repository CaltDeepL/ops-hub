# Task 07: 取り残しの回収（スイーパー + 脱出ハッチ）

| 項目 | 内容 |
|---|---|
| 目的 | プロセス停止で残った `running` の run と、生存セッションが握り続ける advisory lock を回収できるようにする |
| 完了条件 | stale run の自動回収と `unlock` サブコマンドが実装され、`make verify` が PASS する |
| ステータス | 完了（`make verify` PASS、Rust テスト 49件 green） |

## 1. 実施内容

### 背景：2種類の取り残し

| 対象 | 原因 | 排他への影響 | 回収 |
|---|---|---|---|
| `running` のまま残った `runs` | 実行中にプロセス終了 | なし | 自動 |
| advisory lock | セッションが生存したまま停止 | あり | 手動 |

### `src/recovery.rs`

- `sweep_stale_runs` — 閾値を超えて `running` のまま残った run を `failed` に更新
- `lock_key_parts` — 64bit advisory lock key を `pg_locks` の `classid` / `objid` に分解
- `lock_holder` — lock 保持セッションの PID・state・idle 時間・接続元を取得
- `terminate_holder` — `pg_terminate_backend` で保持セッションを切断

### `src/config.rs`

`RUN_STALE_AFTER_SECS` を追加。既定値は600秒。60秒未満または非有限値は拒否する。

### `src/service/run_service.rs`

`start()` の lock 取得前に stale run sweep を実行する。sweep に失敗しても巡回自体は継続し、warn log のみ出す。

```text
start
  ↓
sweep stale runs
  ↓
advisory lock
  ↓
run
```

### `src/main.rs`

`unlock` サブコマンドを追加。`--force` 指定時も、idle 時間が閾値未満の場合は切断しない。

```bash
ops-hub unlock                                # 状態確認のみ
ops-hub unlock --force                        # lock holder を切断
ops-hub unlock --force --min-idle-secs 120    # 最小 idle 時間を指定
```

### SQLx metadata

`sweep_stale_runs` で新しい compile-time query を追加したため `.sqlx` を再生成した（`make sqlx-prepare` → `query data written to .sqlx`）。その後 `make verify` 内の `cargo sqlx prepare --check` も PASS。

### 検証結果

```text
DATABASE_URL guard         PASS
Ruleset contract           PASS
cargo fmt --check          PASS
cargo clippy -D warnings   PASS
cargo sqlx prepare --check PASS
cargo test                 PASS
```

```text
unit tests       29 passed
recovery tests   11 passed
run lock tests    4 passed
runs tests        5 passed

total            49 passed
failed            0
```

recovery test で確認した内容：

- stale run を `failed` に更新
- 閾値内の run は変更しない
- 完了済み run は対象外
- 複数 run の一括回収
- sweep 後の finish が空振りする
- lock 未保持時は `None`
- lock holder を取得できる
- 上位 bit を含む lock key を扱える
- session terminate により lock が解放される
- 存在しない PID の terminate が `false`

## 2. 設計判断

### stale run は自動回収

閾値を超えた `running` は `failed` に倒す。後から本来の処理が終了しても、既存の finish query が `WHERE status = 'running'` を持つため `failed` を上書きしない。

### advisory lock は手動解除

lock holder の切断は正常実行中のセッションを殺す可能性があるため、自動化しない。`inspect → idle 判定 → --force → terminate` という手順を要求する。

### sweep は run 開始前に行い、失敗しても run を止めない

常駐 scheduler を持たないため、定期 cleanup を実行できる場所が `POST /v1/runs` の入口しかない。stale run の整理は観測情報の修復であり排他制御そのものではないので、sweep の失敗で本来の監視 run を止めない。

### `pg_locks` は runtime query

PostgreSQL system catalog の version 差異を `.sqlx` metadata に固定しないため、`lock_holder` は `sqlx::query` を使用する。

## 3. つまずいた点と教訓

| # | 事象 | 対処 |
|---|---|---|
| 1 | config test の `[60.0, 600.0, 3600.0]` で `is_finite()` の型を決定できない | `f64` を明示 |
| 2 | `run_stale_after_secs` 追加で既存 test の `Config` initializer が不足 | 600秒を追加 |
| 3 | 日本語 test 名中の `None` / `PID` が `non_snake_case` warning になる | `clippy -D warnings` に備えて test 名を修正 |
| 4 | 最初の `make verify` が `cargo fmt --check` で停止 | `cargo fmt --all` を適用して再実行 |

**教訓：日本語 test 名でも、英大文字を含むと `non_snake_case` の対象になる**（Task 05 の「日本語名なら warning は出ない」は英大文字を含まない場合に限る）。

## 4. 再現コマンド

```bash
cd back_cargo
docker compose up -d
cargo sqlx migrate run
cd ..

make sqlx-prepare    # compile-time query を追加・変更した場合
DATABASE_URL=postgres://ops_hub:ops_hub@localhost:5433/ops_hub make verify

# unlock の動作確認（back_cargo で）
cargo run -- unlock
cargo run -- unlock --force --min-idle-secs 120
```

## 5. 次タスクへの引き継ぎ

- 次は Task 08。`run_service::perform` の実処理（targets の取得 → probe → checks の記録）を実装する。現在の `perform` は `Ok(0)` を返す空実装
- 本番環境で `unlock` を実行する運用経路は未確定。Render Shell が利用できない場合は、管理用 endpoint または GitHub Actions `workflow_dispatch` 等を別途検討する
- Task 06 で Task 07 に持ち越した以下は**本タスクでも未実装**のまま：
  - `POST /v1/runs` の rate limit（`runs_test.rs` ④、`AppError::TooManyRequests` は未使用）
  - 回収成功時の `recovered: true` レスポンス
