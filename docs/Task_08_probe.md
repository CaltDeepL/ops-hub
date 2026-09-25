# Task 08: 対象の巡回（targets 取得 → probe → checks 記録）

| 項目 | 内容 |
|---|---|
| 目的 | `run_service::perform` の空箱を埋め、有効な target を巡回して結果を `checks` に記録する |
| 完了条件 | 有効な target を巡回して probe 結果を `checks` に永続化でき、`make verify` が PASS する |
| 実装範囲外 | `target_states` の更新・状態遷移・インシデント・通知（Task 09 以降）、target 登録 API（F-1） |
| ステータス | 完了（`make verify` PASS、Rust テスト 72件 green、ローカル実行で実 HTTPS 先への probe を確認） |

## 1. 実施内容

Task 06 のコメントが「タスク8で `targets` の取得・probe・`checks` の記録が入り、タスク9以降で状態遷移とインシデント生成が乗る」と範囲を指定していたので、それに従った。**`target_states` には触っていない。**

### 追加・変更したファイル（`back_cargo/` 配下）

| ファイル | 種別 | 内容 |
|---|---|---|
| `src/masking.rs` | 新規 | エラー文言中の URL からユーザ情報・クエリを伏せ、512文字に詰める |
| `src/provider/mod.rs` | 新規 | `provider` モジュール |
| `src/provider/probe.rs` | 新規 | `build_client` / `probe` / `CheckResult` / `ProbeOutcome` |
| `src/repository/target_repo.rs` | 新規 | `Target` / `list_enabled`（有効な target を名前順） |
| `src/repository/check_repo.rs` | 新規 | `insert`（`checks` へ1行） |
| `tests/probe_test.rs` | 新規 | probe 単体9件 + DB 経由の通し5件 |
| `src/service/run_service.rs` | 変更 | `perform` の実装。`execute` が `AppState` を受け取るよう変更 |
| `src/state.rs` | 変更 | `AppState` に `http: reqwest::Client` を追加 |
| `src/config.rs` | 変更 | `PROBE_CONCURRENCY`（既定4、1以上）を追加 |
| `src/main.rs` | 変更 | `serve` で probe 用クライアントを作って `AppState` に渡す |
| `src/lib.rs` / `src/repository/mod.rs` | 変更 | モジュール登録 |
| `tests/runs_test.rs` | 変更 | 新しい `Config` / `AppState` のフィールドに追従 |
| `.env.example` | 変更 | `PROBE_CONCURRENCY` を追記 |
| `.sqlx/` | 追加 | `list_enabled` / `check_repo::insert` の offline metadata |

`Cargo.toml` は変更なし（`reqwest` は既に依存にある）。

### probe の分類

| 結果 | 条件 | `status_code` | `error_detail` |
|---|---|---|---|
| `success` | `expected_status` と一致。`degraded_threshold_ms` 超過なら `degraded = true` | あり | なし |
| `http_error` | 応答はあったがステータス不一致 | あり | 期待値と実際の値 |
| `timeout` | `timeout_ms` 以内に応答ヘッダが返らない | なし | マスク済みのエラー連鎖 |
| `connection_error` | DNS・TCP・TLS など接続段階の失敗 | なし | マスク済みのエラー連鎖 |

### 検証結果

`make verify`（リポジトリ直下、ローカル PostgreSQL 17）：exit 0

```text
unit tests       38 passed   （masking 7件・probe 2件を追加）
probe tests      14 passed   （新規）
recovery tests   11 passed
run lock tests    4 passed
runs tests        5 passed

total            72 passed
failed            0
```

probe tests で確認した内容：

- 期待どおりのステータスなら success／違えば http_error
- `expected_status` に一致すれば 5xx でも success
- 閾値超えの success は degraded
- 時間切れなら timeout（degraded にはしない）
- 繋がらなければ connection_error
- HEAD でも判定できる
- リダイレクト（308）を追わず http_error になる
- エラー詳細にクエリ文字列・パスワードを残さない
- `POST /v1/runs` → 有効な target 3件が `checks` に記録され、run は `completed`・`targets_checked = 3`
- 無効な target は巡回しない
- 対象一覧は名前順（大文字小文字を区別しない）
- `started_at` が probe 開始時刻として整合する
- 512文字を超えるエラー詳細も詰めて記録できる（`checks_error_detail_len` で落ちない）

ローカルで `serve` を起動し、実際の HTTPS 先で確認した結果（確認後に target と checks は削除済み）：

| target | result | status_code | error_detail |
|---|---|---|---|
| `https://example.com/` | success | 200 | — |
| `https://example.com/definitely-missing?token=secret` | http_error | 404 | 期待したステータス 200 に対して 404 が返りました |
| `https://ops-hub-e2e.invalid/?key=secret` | connection_error | — | `error sending request for url (https://ops-hub-e2e.invalid/?***): client error (Connect): dns error: ...` |

## 2. 設計判断

### リダイレクトを追わない（要確認）

`build_client` に `redirect::Policy::none()` を設定した。追ってしまうと「301 を返すようになった」という変化が `expected_status = 200` のまま success に見え、監視として意味を失う。リダイレクトが正常な対象は `targets.expected_status` に 301 等を設定する運用になる。追う方が実運用に合うなら `Policy::limited(5)` へ変える。既存の設計文書に記述が見当たらなかったので、監視として厳しい側に倒した。

### ボディを読まない（要確認）

ステータス行とヘッダまでで判定を打ち切る。`duration_ms` は実質 TTFB で、全 body 転送時間ではない。コールドスタート検出（F-2）には TTFB の方が素直に効くという判断。「応答を返し切れるか」まで見たい場合は変更が要る。

### `probe` は `Result` を返さない

対象が落ちていることは**このシステムにとっての正常系**。`?` で上へ投げると、1件が落ちているだけで1巡全体が `failed` になり、他の対象の観測結果まで失われる。`perform` が `Err` を返すのは「対象一覧を引けなかった」場合だけ。

`checks` の INSERT が1件失敗した場合も、その件を諦めて巡回を続ける。戻り値（`runs.targets_checked`）は記録できた件数なので、対象数とずれていれば記録漏れがあったと分かる（warn ログも出す）。

### 並行度を `PROBE_CONCURRENCY`（既定4）で絞る

逐次だと対象数 × `timeout_ms`（最大120秒）が1巡の最悪時間になり、5〜6件で `RUN_STALE_AFTER_SECS`（既定600秒）を超えて Task 07 のスイーパーに倒される。一方で無制限に `spawn` すると無料枠から外向き接続を張り過ぎる。

**DB コネクションは追加で消費しない。** `JoinSet` で spawn するタスクは probe（HTTP）だけを行い、`checks` の INSERT は結果を回収するループが逐次に実行する。プール既定は5本で、うち1本は `RunLock` が握ったままなので、probe ごとにコネクションを取ると簡単に枯れる。

### HTTP クライアントを `AppState` で共有する

`reqwest::Client` は内部が `Arc` でコネクションプールを持つ。リクエストごとに作ると TLS ハンドシェイクを毎回やり直すので、`serve` 起動時に1つ作って使い回す。タイムアウトは target ごとに違うので、クライアントではなくリクエスト単位で設定する。

### `checks.started_at` を SQL 側で逆算する

`now() - duration_ms * interval '1 millisecond'` で probe 開始時刻を復元する。Rust 側で `timestamptz` を組み立てるには `chrono` を直接の依存に足す必要があり（現状は sqlx のフィーチャー経由のみ）、Task 07 で `idle_secs` を SQL 側で秒数に落としたのと同じ方針を踏襲した。INSERT は probe 直後なので誤差はミリ秒単位。

下書き段階では `make_interval(secs => duration_ms/1000)` を想定していたが、同じパラメータを `integer` と `double precision` の両方として使うと型推論がぶれるため、`$3::integer * interval '1 millisecond'` に変えた。

### ENUM を `$4::text::check_result` で渡す

`$4::check_result` と直接書くと PostgreSQL が `$4` 自体を `check_result` 型と推論し、sqlx が ENUM 対応の Rust 型を要求してくる。一度 `text` に落とせば素の `&str` で渡せる。`run_repo` の「この層では SQL のリテラルとしてしか扱わない」方針を維持するための書き方。`CheckResult::as_str` とマイグレーションの ENUM 値の一致はユニットテストで固定した。

### masking を独立モジュールにする

`config.rs` の `redact_url` は起動ログの `DATABASE_URL` 専用でユーザ名を残す（どの資格情報で繋いでいるか分からないと調査にならないため）。`checks.error_detail` は残す理由が無いのでユーザ名ごと落とす。用途が違うので統合しない。

`reqwest` のエラー表示はリクエスト URL をそのまま含むため、署名付き URL の署名や API キーがクエリに載っていると90日残る列と Slack 通知の両方に流れる。**伏せ漏れより伏せ過ぎ**の方針で、クエリ文字列は中身を見ずに `?***` に置き換える。伏せてから512文字に詰める（逆順だと、詰めた位置で URL が途中から始まり検出から漏れる）。

### エラー詳細は `source()` の連鎖をたどる

`reqwest::Error` の表示は最上位（`error sending request for url (...)`）だけで、`dns error` や `connection refused` などの原因は `source()` の先にある。連鎖をたどって1行にまとめないと、調査に使えない文言しか残らない。

### テストを2層に分ける

`targets` には `targets_url_https` CHECK があり、平文 HTTP のテストサーバは DB 経由では叩けない。

1. `probe` 単体：`Target` を Rust 側で組み立てて CHECK を迂回し、ローカルの axum サーバへ実際に投げる
2. 通し：`https://` のまま**誰も listen していないポート**を指し、`connection_error` が記録されることで配線を確かめる

TLS 付きのテストサーバを立てれば1本にできるが、証明書の用意とクライアント側の検証無効化が要る割に、得られるのは2で既に取れている確証だけなので見送った。TLS を含む実経路はローカル実行で確認した（1章）。

## 3. つまずいた点と教訓

下書き時点で想定していた失敗点は、いずれも実際には起きなかった。

| # | 想定 | 実際 |
|---|---|---|
| 1 | `$5::text::check_result` の型推論が通らない | 通った（パラメータ番号は `$4` に変更） |
| 2 | `while ... && let Some(target) = queue.next()` の let-chain | edition 2024 / Rust 1.96 でそのまま通った |
| 3 | `閾値超えのsuccessはdegraded` が CI で不安定 | ローカルでは安定。timeout を5秒にして余裕を取っている |
| 4 | 閉じたポートを別プロセスが掴む | 発生せず（理論上の可能性として残る） |
| 5 | cast 系の clippy lint | `i32::try_from` / `u64::try_from` に寄せたので出なかった |

実際に踏んだもの：

- **`started_at` の逆算式を変えた。** 下書きの `make_interval(secs => duration_ms/1000)` では、同じパラメータを整数と浮動小数の両方として使うことになり、sqlx が推論する Rust 側の型がぶれる。`$3::integer * interval '1 millisecond'` にすると型が1つに決まる
- **E2E 確認で `cargo run` がリポジトリ直下で走り失敗した。** `Cargo.toml` は `back_cargo/` にある（Task 20 と同じ罠）。`--manifest-path back_cargo/Cargo.toml` を付ける
- **axum の `Redirect::permanent` は 301 ではなく 308 を返す。** リダイレクト非追従テストの期待値は 308

## 4. 再現コマンド

```bash
# ローカル PostgreSQL と migration
cd back_cargo && docker compose up -d db && cargo sqlx migrate run && cd ..

# 新しい compile-time query を追加したので .sqlx の再生成が必要
make sqlx-prepare

# 品質ゲート
DATABASE_URL=postgres://ops_hub:ops_hub@localhost:5433/ops_hub make verify

# probe だけ
cd back_cargo && cargo test --test probe_test && cd ..
```

実際の target で確かめるとき（ローカル DB。終わったら消す）：

```bash
export DATABASE_URL=postgres://ops_hub:ops_hub@localhost:5433/ops_hub
psql "$DATABASE_URL" -c "select current_database()"   # ops_hub が返ること

psql "$DATABASE_URL" -c "INSERT INTO targets (name, url, severity) VALUES ('e2e-ok', 'https://example.com/', 'sev3')"

PORT=18099 LOG_FORMAT=pretty cargo run --manifest-path back_cargo/Cargo.toml -- serve &
curl -i -X POST http://localhost:18099/v1/runs        # 202 started
psql "$DATABASE_URL" -c "select t.name, c.result, c.status_code, c.duration_ms, c.degraded, c.error_detail
                         from checks c join targets t on t.id = c.target_id order by c.id desc limit 5"
kill %1

# 後片付け（checks は targets を RESTRICT で参照しているので先に消す）
psql "$DATABASE_URL" -c "DELETE FROM checks WHERE target_id IN (SELECT id FROM targets WHERE name LIKE 'e2e-%');
                         DELETE FROM targets WHERE name LIKE 'e2e-%'"
```

## 5. 次タスクへの引き継ぎ

- **Task 09**：`checks` を読んで `target_states` を更新する状態遷移（`consecutive_failures`、`up` / `down`、フラッピング抑制30分）。`current_incident_id` と `incidents`（migration 0003 済み）もここから
  - `perform` のループは「probe 結果を1件受け取るたびに記録する」形なので、状態遷移は `check_repo::insert` の直後に同じループで足すのが自然。`ProbeOutcome.result` が success 以外なら失敗として数える
  - `degraded` は状態遷移に影響させない（`checks.degraded` の COMMENT どおり）
  - `Target` に `severity` をまだ読んでいない。通知で使う段になったら `list_enabled` の SELECT に足す（`.sqlx` の再生成が要る）
- **要確認の判断2件**（リダイレクト非追従・ボディを読まない）は仕様として採るか判断が要る。変える場合は `provider/probe.rs` の `build_client` と `probe` だけで閉じる
- `targets` を登録する手段が無いまま（F-1 の API 未実装）。現状は `psql` で直接入れる運用
- `POST /v1/runs` の rate limit と `recovered: true` は未実装のまま（Task 06・07 からの持ち越し）
- 本番（Render / Neon）には未デプロイ。seed（`0005_seed_targets`）も未作成なので、デプロイしても対象0件で `targets_checked = 0` になる
