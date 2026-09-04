# タスク04：`AppError` と problem+json

| 項目 | 内容 |
|---|---|
| 上位ドキュメント | ops-hub-detail v1.0 3章（エラー設計）／ 10章 タスク4 |
| 完了条件 | SQLSTATE分類のテストが通る。5xxで `trace_id` のみ返る |
| ステータス | ユニットテスト22件 green（2026-09-05）。`x-request-id` の往復のみ未確認（api イメージの再ビルド待ち） |

> 実測（2026-09-05）：`cargo test` 22件すべて green。5章の「想定される修正点」で
> 実際に踏んだのは clap の feature 不足のみで、sqlx 0.9 のバリアント名・`HeaderName::from_static`
> の const 文脈・`Future` の prelude はいずれも修正不要だった。

## 1. このタスクで作ったもの

| ファイル | 役割 |
|---|---|
| `src/error.rs` | `AppError` / `ProblemDetails` / SQLSTATE分類 / `OnConstraint` |
| `src/trace_id.rs` | リクエスト単位の `trace_id` を task-local で持ち回るミドルウェア |
| `src/lib.rs` | `error` / `trace_id` の登録、ミドルウェアの適用 |
| `Cargo.toml` | `thiserror` / `uuid` を追加、`clap` に `env` feature を追加 |

## 2. 設計判断

| # | 判断 | 根拠 |
|---|---|---|
| 1 | 分類を `classify(sqlstate, constraint) -> DbErrorClass` の純粋関数に切り出す | `sqlx::Error::Database` は外部から組み立てられない（`PgDatabaseError` のフィールドが private）。関数を分けないと、分類のテストにDBが要る。**完了条件「SQLSTATE分類のテストが通る」を DB 無しで満たすための構造** |
| 2 | `trace_id` を task-local に置く | `IntoResponse` はリクエストの extension を見られない。ハンドラの引数に `TraceId` を足して回る案もあるが、全ハンドラに波及する。task-local なら `AppError` 側だけで完結する |
| 3 | 受信した `x-request-id` は文字種と長さを検証してから使う | 外部の値をそのまま構造化ログに載せると、改行を混ぜてログを壊される。英数と `-` `_`、64文字までに制限 |
| 4 | DB由来のうち接続系（class 08 / 57、`PoolTimedOut` など）は 503 | 詳細設計2.2 の「DB接続不可 → 503」。500 と混ぜると、監視側で「落ちている」と「壊れている」が区別できない |
| 5 | 422 の `detail` に制約名を入れる | 単一利用者のツールであり、`targets_url_https` と出たほうが原因に即たどり着ける。秘匿値ではないので N-8 に抵触しない |
| 6 | `ExpectedDuplicate` がハンドラまで届いたら 500 + `warn` | 23505 の2件は repository 層で握りつぶすのが正しい（詳細設計3.3）。ここまで来たのは実装漏れなので、黙って 4xx にせず気づけるようにする |
| 7 | ロック競合のバリアントは作らない | 詳細設計3.2。成功型 `RunOutcome` で表す。タスク6で実装する |

## 3. SQLSTATE 分類表（実装した対応）

| SQLSTATE | 制約名 | 分類 | HTTP | 備考 |
|---|---|---|---|---|
| 23505 | `events_source_idempotency_key` | ExpectedDuplicate | 500※ | 正しくは repository で握りつぶし 202 `duplicate` |
| 23505 | `outbox_dedupe_key` | ExpectedDuplicate | 500※ | 正しくは `debug` ログのみで握りつぶす（D-4） |
| 23505 | `incidents_one_open_per_target` | LogicViolation | 500 | `warn` を残す（D-5） |
| 23505 | その他（`targets_name_key` 等） | Unprocessable | 422 | |
| 23514 / 23503 / 23502 / 22P02 | — | Unprocessable | 422 | CHECK / FK / NOT NULL / 不正な入力値 |
| 08xxx / 57xxx | — | Unavailable | 503 | 接続断・管理者による切断 |
| その他 | — | Unexpected | 500 | |
| `RowNotFound` | — | — | 404 | |

※ ハンドラまで到達した場合のフォールバック。到達しないことが正常。

制約名はタスク02・03の検証スクリプトで実際に観測した値をそのまま使っている。
**マイグレーションで制約名を変えたら `classify` も変える**（テストが落ちる形になっている）。

## 4. テスト（`cargo test` で実行）

| モジュール | ケース | 内容 |
|---|---|---|
| `error::tests` | 7 | 分類：冪等キー重複／dedupe_key重複／二重インシデント／その他の一意違反／CHECK系4種／接続系／未知コード |
| `error::tests` | 2 | ステータス対応、`RowNotFound` → 404 |
| `error::tests` | 3 | 5xxは `detail` 無し・`trace_id` あり・Webhook URLが漏れない／4xxは `detail` を保つ／401に `WWW-Authenticate` |
| `trace_id::tests` | 4 | ヘッダ検証（正常・改行・空白・65文字）、スコープ内外の `current()` |

## 5. つまずいた点と教訓

| # | 事象 | 原因 | 対処 |
|---|---|---|---|
| 1 | `#[arg(env = ...)]` がコンパイルエラー | `clap` の `env` 属性は `env` feature に含まれる。`derive` だけでは有効にならない | `clap = { version = "4", features = ["derive", "env"] }` |
| 2 | `curl` のレスポンスに `x-request-id` が返らない | `cargo test` はホスト側でビルドしているが、叩いた先（`localhost:8081`）は**再ビルド前のイメージで動いている api コンテナ**。ミドルウェアを足したコードが入っていない | `docker compose up --build -d api` で作り直してから再確認 |

**教訓：`cargo test` が通ったことと、コンテナが新しいコードで動いていることは別。**
compose 越しに動作確認するタスクでは、確認の前に必ずイメージを作り直す。

事前に想定していた修正点（sqlx 0.9 のバリアント名、`HeaderName::from_static` の const 文脈、
`Future` の prelude、`code()` の `Cow`）は、いずれも修正不要だった。

## 6. 再現コマンド

```bash
cargo test error::            # 分類とproblem+json（12件）
cargo test trace_id::         # ヘッダ検証（4件）
cargo test                    # 全件（22件）

# ★ コンテナ側の確認は必ず再ビルドしてから
docker compose up --build -d api

curl -i -H 'x-request-id: manual-check-001' http://localhost:8081/health
# → レスポンスヘッダに x-request-id: manual-check-001 が返ること

docker compose stop db
curl -i http://localhost:8081/health    # 503
docker compose logs api | tail -5       # ログの trace_id がヘッダと一致すること
docker compose start db
```

実測（2026-09-05）：`cargo test` 22件 green。DB停止時の `/health` は 503 を返すことを確認済み。
`x-request-id` の往復は、再ビルド後に確認する。

## 7. 次タスクへの引き継ぎ

- タスク5は `RunLock`（advisory lock）。詳細設計1.2 の「ロック保持コネクションで他のクエリを流さない」を型で担保する
- `RunLock::try_acquire` の戻りは `Result<Option<Self>, sqlx::Error>`。**取得失敗は `AppError` にしない**（3.2）
- 残る宿題1（Neonで `tcp_keepalives_idle` が設定できるか）はタスク5で確認する
- `repository` 層を作り始めたら、`OnConstraint::is_unique_violation_on` を使って
  `outbox_dedupe_key` / `events_source_idempotency_key` の重複を握りつぶす（分類が `ExpectedDuplicate` のものは、ハンドラまで上げない）

