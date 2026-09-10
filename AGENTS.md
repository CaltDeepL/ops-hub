# ops-hub — Codex 実装ルール

## 役割

Codex は ops-hub の **実装担当** とする。

Claude Code が仕様化・設計判断・レビューを担当し、Codex は承認済み task doc を最小差分で実装する。

「より良い設計を思いついた」という理由だけで、承認済み設計を変更してはならない。

## 作業前に読む順番

対象タスクを実装する前に、最低限次を読む。

1. 対象の `docs/task-ID-*.md`
2. `docs/ai/PROJECT.md`
3. `docs/ai/WORKFLOW.md`
4. タスクから参照される ADR
5. 対象コード、テスト、migration、関連設定
6. 必要な場合のみ上位設計文書

設計文書の優先順位は既存 `docs/implementation-plan.md` に従う。

1. 要件定義 v0.2
2. 基本設計 v1.0
3. 詳細設計 v1.0
4. 要件定義 v0.1 は旧版

ただし、詳細設計が基本設計を明示的に修正している箇所は詳細設計を優先する。

## 現在の実装基準

Task 01〜06 は完了済みとして扱う。現在の入口仕様は次のとおり。

- `POST /v1/runs` は advisory lock を取得できれば run を `running` で作成し、HTTPは `202 Accepted` を即返す。
- 実処理は `tokio::spawn` 側で行う。
- lock 競合はエラーではなく `200 OK` + `already_running`。
- lock競合時の `run_id` は race により `null` を許容する。
- run 完了記録を行ってから advisory lock を解放する。
- `AppState` のDBフィールド名は `db`。Task 06 時点で `config: Arc<Config>` を持つ。
- Task 07 の回収成功時だけレスポンスへ `recovered: true` を追加する。

Task 07 では次を壊さない。

- stale lock の特定は `pg_locks.classid` / `objid` を組み合わせて bigint key を復元する。
- `objid = $1` だけで lock を特定しない。
- `pg_terminate_backend` 後の再取得は1回だけ。
- 回収不能・権限不足は `AlreadyRunning` にフォールバックする。

## Spec Deviations — 最重要ルール

実装に必要な変更が task doc の `設計判断・不変条件` と矛盾する場合、**コードで勝手に解決しない**。

1. task doc の `## Spec Deviations` に「何を変える必要があるか」「なぜか」を記録する。
2. その設計変更に依存する実装を止める。
3. task status を `blocked` にする。
4. Claude architect が仕様を更新するまで再開しない。

設計の黙示的変更は禁止。

## Rust / Axum

- handler は HTTP I/O と入力境界に集中させる。
- use case の組み立ては service に置く。
- DBアクセスは repository に置く。
- 状態遷移など決定論的なロジックは domain の純粋関数を優先する。
- 外部通知は provider 境界へ置く。
- Probe は既存設計どおり安易に trait 化しない。HTTP挙動の検証は wiremock を使う方針を維持する。
- 不要な抽象化、将来用フレームワーク、今回使わない汎用化を追加しない。

## PostgreSQL / SQLx

- 適用済み migration ファイルは編集しない。変更は必ず新規 migration で行う。
- `NOT NULL` 追加、DROP/RENAME、型縮小などの破壊的変更は、task doc に段階適用・backfill・互換性を明記してから実装する。
- migration や `query!` / `query_as!` 等を変更し SQLx metadata が変わる場合は `.sqlx/` を更新し、差分へ含める。
- `.sqlx/` 更新用コマンドは `make sqlx-prepare` を使う。
- 複数のDB更新が同一不変条件を構成する場合は transaction に含める。
- 排他・冪等性はアプリの偶然ではなく、DB制約・transaction・lock の適切な層で担保する。

## ops-hub固有の信頼性

- retry、timeout、二重起動、クラッシュ途中終了を通常ケースとして設計する。
- `POST /v1/runs` は冪等APIではないが、同一対象の重複チェックを並行実行させない。
- `200 already_running` を安易に `409` へ変更しない。デッドマンスイッチを誤失敗させる。
- incident / outbox / notification は重複生成・重複送信の防止境界を明示する。
- 5xx やログにDB接続情報、token、Webhook URL等を漏らさない。

## 本番DB禁止

Claude/Codexによる実装・テスト・SQLx metadata生成では Neon 本番DBへ接続しない。

最終検証は、既存ローカルDocker DBなど許可されたホストのみを使用する。

標準例:

```bash
export DATABASE_URL="postgres://ops_hub:ops_hub@localhost:5433/ops_hub"
```

`make verify` / `make sqlx-prepare` はホストを検査し、非ローカルDBを拒否する。

`.env` の秘密情報を読んで回避してはならない。

## 最終品質ゲート

最終検証は必ず次の1コマンド。

```bash
make verify
```

狭いテストは実装中に実行してよいが、最終検証を置き換えない。

`make verify` が実際に成功していないのに「検証済み」「green」と書かない。

## 実装完了時

managed task（Q系列およびTask 07以降）は task doc の `Implementation Record` を更新する。

- 変更ファイル
- 実装上の判断
- migration / API影響
- `make verify` の実際の結果
- 制約・残課題

Acceptance Criteria は、実際に満たした項目だけ `[x]` にする。

Claude の `Review Record` や READY 判定は書き換えない。

## Git操作

通常フローでは次を行わない。

- `git commit`
- `git commit --amend`
- `git rebase`
- `git push`
- `git reset --hard`
- `git clean -fd`
- deploy

READY後のコミットは人間が行う。
