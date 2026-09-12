# Task 08: 対象の巡回（targets 取得 → probe → checks 記録）

`run_service::perform` の空箱を埋めるタスク。タスク6のコメントが「タスク8で `targets` の取得・probe・`checks` の記録が入り、タスク9以降で状態遷移とインシデント生成が乗る」と範囲を指定していたので、それに従いました。

**`target_states` には触っていません。** 状態遷移・インシデント・通知はタスク9以降です。

## 1. 追加・変更したファイル

| ファイル | 配置先 | 種別 |
|---|---|---|
| `masking.rs` | `back_cargo/src/masking.rs` | 新規 |
| `provider/mod.rs` | `back_cargo/src/provider/mod.rs` | 新規 |
| `provider/probe.rs` | `back_cargo/src/provider/probe.rs` | 新規 |
| `repository/target_repo.rs` | `back_cargo/src/repository/target_repo.rs` | 新規 |
| `repository/check_repo.rs` | `back_cargo/src/repository/check_repo.rs` | 新規 |
| `tests/probe_test.rs` | `back_cargo/tests/probe_test.rs` | 新規 |
| `lib.rs` | `back_cargo/src/lib.rs` | 差し替え |
| `state.rs` | `back_cargo/src/state.rs` | 差し替え |
| `config.rs` | `back_cargo/src/config.rs` | 差し替え |
| `repository/mod.rs` | `back_cargo/src/repository/mod.rs` | 差し替え |
| `service/run_service.rs` | `back_cargo/src/service/run_service.rs` | 差し替え |
| `main.rs` | `back_cargo/src/main.rs` | 差し替え |
| `tests/runs_test.rs` | `back_cargo/tests/runs_test.rs` | 差し替え |

`Cargo.toml` は変更不要です。`reqwest` は既に入っており、コメントにも「タスク8の provider/probe.rs でも同じクライアントを使う」と書かれていました。

## 2. 判断が要る点（先に見てください）

### リダイレクトを追わない

`build_client` に `redirect::Policy::none()` を設定しました。追ってしまうと「301 を返すようになった」という変化が `expected_status = 200` のまま success に見え、監視として意味を失うためです。リダイレクトが正常な対象は `targets.expected_status` に 301 を設定する運用になります。

追う方が実運用に合うと判断される場合は `Policy::limited(5)` へ変えてください。既存の設計文書にこの記述が見当たらなかったので、監視として厳しい側に倒しています。

### ボディを読まない

ステータス行とヘッダまでで判定を打ち切ります。したがって `duration_ms` は実質 TTFB で、全body転送時間ではありません。コールドスタート検出（F-2）には TTFB の方が素直に効くという判断ですが、「応答を返し切れるか」まで見たい場合は変更が要ります。

### `checks.started_at` を SQL 側で逆算している

`now() - make_interval(secs => duration_ms/1000)` で probe 開始時刻を復元しています。Rust 側で `timestamptz` を組み立てるには `chrono` を直接の依存に足す必要があり（現状は sqlx のフィーチャー経由のみ）、タスク7で `idle_secs` を `f64` で受けたのと同じ方針を踏襲しました。INSERT は probe 直後なので誤差はミリ秒単位です。

## 3. 設計上の判断

### `probe` は `Result` を返さない

対象が落ちていることは **このシステムにとっての正常系** です。`?` で上へ投げると、1件が落ちているだけで1巡全体が `failed` になり、他の対象の観測結果まで失われます。`perform` が `Err` を返すのは「対象一覧を引けなかった」場合だけです。

`checks` の INSERT が1件失敗した場合も、その件を諦めて巡回を続けます。戻り値の件数は失敗分を含まないので、`runs.targets_checked` と対象数がずれていれば記録漏れがあったと分かります。

### 並行度を `PROBE_CONCURRENCY`（既定4）で絞る

逐次だと対象数 × `timeout_ms`（最大120秒）が1巡の最悪時間になり、5〜6件で `RUN_STALE_AFTER_SECS`（既定600秒）を超えてタスク7のスイーパーに倒されます。一方で無制限に `spawn` すると無料枠から外向き接続を張り過ぎます。

**DB コネクションは追加で消費しません。** spawn するタスクは probe だけを行い、`checks` の INSERT は結果を回収するループが逐次に実行します。プール既定は5本で、うち1本は `RunLock` が握ったままなので、probe ごとにコネクションを取ると簡単に枯れます。

### ENUM を `$5::text::check_result` で渡している

`$5::check_result` と直接書くと PostgreSQL が `$5` 自体を `check_result` 型と推論し、sqlx が ENUM 対応の Rust 型を要求してきます。一度 `text` に落とせば素の `&str` で渡せます。`run_repo` の「この層では SQL のリテラルとしてしか扱わない」方針を維持するための書き方です。

### masking を独立モジュールにした

`config.rs` にも `redact_url` がありますが、あちらは起動ログの `DATABASE_URL` 専用でユーザ名を残します（どの資格情報で繋いでいるか分からないと調査にならないため）。`checks.error_detail` は残す理由が無いのでユーザ名ごと落とします。用途が違うので統合していません。

`reqwest` のエラー表示はリクエスト URL をそのまま含むため、署名付き URL の署名や API キーがクエリに載っていると90日残る列と Slack 通知の両方に流れます。**伏せ漏れより伏せ過ぎ**の方針で、クエリ文字列は中身を見ずに丸ごと落としています。

## 4. テストを2層に分けた理由

`targets` には `targets_url_https` CHECK があり `https://` で始まる URL しか入りません。ローカルに立てたテストサーバは平文 HTTP なので、**DB を経由する経路ではテストサーバを叩けません。**

1. `probe` 単体のテストは `Target` を Rust 側で組み立てて CHECK を迂回し、ローカルの axum サーバへ実際に投げる（success / http_error / expected_status 一致 / degraded / timeout / connection_error / HEAD / マスキング）
2. `targets` → probe → `checks` の通しは、`https://` のまま**誰も listen していないポート**を指し、`connection_error` が1行記録されることで配線を確かめる（記録 / enabled 絞り込み / 名前順 / `started_at` 逆算 / 512文字制約）

TLS 付きのテストサーバを立てれば1本にできますが、証明書の用意とクライアント側の検証無効化が要る割に、得られるのは2で既に取れている確証だけなので見送りました。

テストサーバのポートは `127.0.0.1:0` で OS に選ばせています。固定ポートは並列実行で衝突します。

## 5. 適用手順

`target_repo::list_enabled` と `check_repo::insert` が新しいコンパイル時クエリなので、**`.sqlx` の再生成が必須**です。

```bash
cd back_cargo && docker compose up -d && cargo sqlx migrate run
cd .. && make sqlx-prepare && make verify
```

## 6. 検証状況

**サンドボックスに `cargo` と PostgreSQL が無いため、コンパイルとテスト実行は未確認です。** 波括弧の対応、モジュール登録、既存シグネチャとの整合までしか確認できていません。

### 想定される失敗点

1. **`$5::text::check_result` の型推論。** 通らなければ `#[derive(sqlx::Type)]` で `check_result` に写像する方針へ切り替えることになります（`.sqlx` に ENUM 定義を含める必要が出ます）。
2. **`while running.len() < concurrency && let Some(target) = queue.next()`** の let-chain。2024 edition なので通るはずですが（既存 `runs_test.rs` も `if let ... && ...` を使用済み）、`while` での let-chain が通らなければ `loop` + `break` に展開してください。
3. **`閾値超えのsuccessはdegraded` テスト。** `/lazy` の300ms待ちに対し閾値50msなので余裕はありますが、CI が極端に遅いと `timeout` 側が先に効く可能性があります（timeout は5秒に設定済み）。
4. **`繋がらなければconnection_error` テスト。** ポートを解放してから接続するまでの間に別プロセスが同じポートを掴む理論上の可能性があります。
5. `elapsed_ms` の `i32::try_from` と `timeout_ms.max(0) as u64` で clippy の cast 系 lint が出る可能性。既定の clippy では pedantic 系は無効なので通る想定です。

## 7. 次タスクへの引き継ぎ

- **task-09**: `checks` を読んで `target_states` を更新する状態遷移（`consecutive_failures`、`up`/`down`、フラッピング抑制30分）。`current_incident_id` と `incidents`（migration 0003 済み）もここから。
- `targets` を登録する手段が無いまま（F-1 の API 未実装）。現状 `psql` で直接入れる運用です。probe を実地で確認するには対象を手で入れる必要があります。
- `handler/runs.rs` のコメントにある `recovered: true` はタスク7でも実装しておらず、未着手のままです。
- 品質基盤側の残課題（task-22 §6）は解決済みです。