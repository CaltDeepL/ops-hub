# Task 08: 対象の巡回（targets 取得 → probe → checks 記録）

| 項目 | 内容 |
|---|---|
| 目的 | `run_service::perform` の空箱を埋め、登録済み target を巡回して結果を `checks` に記録する |
| 完了条件 | 有効な target を巡回して probe 結果を `checks` に永続化でき、`make verify` が PASS する |
| 実装範囲外 | `target_states` の更新・状態遷移・インシデント・通知（Task 09 以降） |
| ステータス | **未完了（設計・下書きのみ）。** 下書きのコードはリポジトリに未適用（2026-09-25 時点で `masking.rs` / `provider/` / `target_repo.rs` / `check_repo.rs` / `probe_test.rs` はいずれも存在せず、`perform` は `Ok(0)` のまま）。コンパイル・テストも未実施 |

## 1. 実施内容

Task 06 のコメントが「タスク8で `targets` の取得・probe・`checks` の記録が入り、タスク9以降で状態遷移とインシデント生成が乗る」と範囲を指定していたので、それに従った。**`target_states` には触っていない。**

### 追加・変更予定のファイル

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

`Cargo.toml` は変更不要。`reqwest` は既に入っており、コメントにも「タスク8の provider/probe.rs でも同じクライアントを使う」と書かれている。

### テストの2層構成

`targets` には `targets_url_https` CHECK があり `https://` で始まる URL しか入らない。ローカルに立てたテストサーバは平文 HTTP なので、**DB を経由する経路ではテストサーバを叩けない。**

1. `probe` 単体のテストは `Target` を Rust 側で組み立てて CHECK を迂回し、ローカルの axum サーバへ実際に投げる（success / http_error / expected_status 一致 / degraded / timeout / connection_error / HEAD / マスキング）
2. `targets` → probe → `checks` の通しは、`https://` のまま**誰も listen していないポート**を指し、`connection_error` が1行記録されることで配線を確かめる（記録 / enabled 絞り込み / 名前順 / `started_at` 逆算 / 512文字制約）

TLS 付きのテストサーバを立てれば1本にできるが、証明書の用意とクライアント側の検証無効化が要る割に、得られるのは2で既に取れている確証だけなので見送った。テストサーバのポートは `127.0.0.1:0` で OS に選ばせる（固定ポートは並列実行で衝突する）。

## 2. 設計判断

### 要確認の判断（先に見てほしい点）

**リダイレクトを追わない。** `build_client` に `redirect::Policy::none()` を設定した。追ってしまうと「301 を返すようになった」という変化が `expected_status = 200` のまま success に見え、監視として意味を失う。リダイレクトが正常な対象は `targets.expected_status` に 301 を設定する運用になる。追う方が実運用に合うなら `Policy::limited(5)` へ変える。既存の設計文書に記述が見当たらなかったので、監視として厳しい側に倒した。

**ボディを読まない。** ステータス行とヘッダまでで判定を打ち切る。したがって `duration_ms` は実質 TTFB で、全 body 転送時間ではない。コールドスタート検出（F-2）には TTFB の方が素直に効くという判断だが、「応答を返し切れるか」まで見たい場合は変更が要る。

**`checks.started_at` を SQL 側で逆算する。** `now() - make_interval(secs => duration_ms/1000)` で probe 開始時刻を復元する。Rust 側で `timestamptz` を組み立てるには `chrono` を直接の依存に足す必要があり（現状は sqlx のフィーチャー経由のみ）、Task 07 で `idle_secs` を `f64` で受けたのと同じ方針を踏襲した。INSERT は probe 直後なので誤差はミリ秒単位。

### `probe` は `Result` を返さない

対象が落ちていることは**このシステムにとっての正常系**。`?` で上へ投げると、1件が落ちているだけで1巡全体が `failed` になり、他の対象の観測結果まで失われる。`perform` が `Err` を返すのは「対象一覧を引けなかった」場合だけ。

`checks` の INSERT が1件失敗した場合も、その件を諦めて巡回を続ける。戻り値の件数は失敗分を含まないので、`runs.targets_checked` と対象数がずれていれば記録漏れがあったと分かる。

### 並行度を `PROBE_CONCURRENCY`（既定4）で絞る

逐次だと対象数 × `timeout_ms`（最大120秒）が1巡の最悪時間になり、5〜6件で `RUN_STALE_AFTER_SECS`（既定600秒）を超えて Task 07 のスイーパーに倒される。一方で無制限に `spawn` すると無料枠から外向き接続を張り過ぎる。

**DB コネクションは追加で消費しない。** spawn するタスクは probe だけを行い、`checks` の INSERT は結果を回収するループが逐次に実行する。プール既定は5本で、うち1本は `RunLock` が握ったままなので、probe ごとにコネクションを取ると簡単に枯れる。

### ENUM を `$5::text::check_result` で渡す

`$5::check_result` と直接書くと PostgreSQL が `$5` 自体を `check_result` 型と推論し、sqlx が ENUM 対応の Rust 型を要求してくる。一度 `text` に落とせば素の `&str` で渡せる。`run_repo` の「この層では SQL のリテラルとしてしか扱わない」方針を維持するための書き方。

### masking を独立モジュールにする

`config.rs` の `redact_url` は起動ログの `DATABASE_URL` 専用でユーザ名を残す（どの資格情報で繋いでいるか分からないと調査にならないため）。`checks.error_detail` は残す理由が無いのでユーザ名ごと落とす。用途が違うので統合しない。

`reqwest` のエラー表示はリクエスト URL をそのまま含むため、署名付き URL の署名や API キーがクエリに載っていると90日残る列と Slack 通知の両方に流れる。**伏せ漏れより伏せ過ぎ**の方針で、クエリ文字列は中身を見ずに丸ごと落とす。

## 3. つまずいた点と教訓

下書き時点のサンドボックスに `cargo` と PostgreSQL が無く、コンパイルとテスト実行は未確認。以下は**未検証の予測**。

| # | 想定される失敗点 | 対処方針 |
|---|---|---|
| 1 | `$5::text::check_result` の型推論が通らない | `#[derive(sqlx::Type)]` で `check_result` に写像する方針へ切り替える（`.sqlx` に ENUM 定義を含める必要が出る） |
| 2 | `while running.len() < concurrency && let Some(target) = queue.next()` の let-chain | 2024 edition なので通るはずだが、通らなければ `loop` + `break` に展開 |
| 3 | `閾値超えのsuccessはdegraded` テスト | `/lazy` の300ms 待ちに対し閾値50ms なので余裕はあるが、CI が極端に遅いと `timeout` 側が先に効く可能性（timeout は5秒） |
| 4 | `繋がらなければconnection_error` テスト | ポートを解放してから接続するまでの間に別プロセスが同じポートを掴む理論上の可能性 |
| 5 | `elapsed_ms` の `i32::try_from` と `timeout_ms.max(0) as u64` | clippy の cast 系 lint。既定の clippy では pedantic 系は無効なので通る想定 |

## 4. 再現コマンド

`target_repo::list_enabled` と `check_repo::insert` が新しいコンパイル時クエリなので、**`.sqlx` の再生成が必須**。

```bash
cd back_cargo && docker compose up -d && cargo sqlx migrate run
cd .. && make sqlx-prepare && make verify
```

probe を実地で確認するには、target を `psql` で手動登録してから `POST /v1/runs` を叩く（登録 API は未実装）。

## 5. 次タスクへの引き継ぎ

- **まず本タスクの下書きをリポジトリに適用し、`make verify` を通すこと**（現状未適用）
- Task 09: `checks` を読んで `target_states` を更新する状態遷移（`consecutive_failures`、`up`/`down`、フラッピング抑制30分）。`current_incident_id` と `incidents`（migration 0003 済み）もここから
- `targets` を登録する手段が無いまま（F-1 の API 未実装）。現状 `psql` で直接入れる運用
- `handler/runs.rs` のコメントにある `recovered: true` は Task 07 でも実装しておらず、未着手のまま
- 品質基盤側の残課題（Task 22 §5）は解決済み
