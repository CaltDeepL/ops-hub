# ops-hub — Codex 実装ルール v3

## 役割と正本

開発主体はHumanとする。

- Human: Owner。優先順位、仕様承認、Git履歴、GitHub設定、merge
- Claude: Architect / Context Compiler。設計整理、task doc作成、review
- Codex: Implementation Engine。実リポジトリへの適用、修正、test、検証
- CI: Mechanical Quality Gate
- GitHub Ruleset: Repository Protection

Claudeは実リポジトリへ直接アクセスできない前提とし、Claude生成コードや`implementation/`は未検証の提案として扱う。実装判断の優先順位は次のとおり。

```text
Human承認済みAcceptance Criteria
  ↓
Invariants
  ↓
実リポジトリ
  ↓
Required Tests
  ↓
task docのDesign Decisions
  ↓
Claude生成コード
```

Claude生成コードが実リポジトリと衝突した場合は破棄してよい。「より良い設計」を理由に承認済み仕様を変更しない。

## 読み取り範囲

通常は次だけを読む。

```text
Status: APPROVEDのtask doc
+ MANIFEST.md（存在する場合。未検証の一時成果物）
+ task docのFilesにある変更対象
+ 関連テスト
```

`PROJECT.md`、`WORKFLOW.md`、全ADR、`implementation-plan.md`、要件・設計全文を毎回再読しない。

必要情報が足りない場合だけ、次の順で段階的に広げる。

- L0: approved task doc + MANIFEST + 対象コード + 対象テスト。通常はここで実装する。
- L1: 変更対象の呼び出し元、関連テスト、直接依存module。compile/interface/既存test確認に必要な場合だけ読む。
- L2: 該当ADR、詳細設計、必要な上位設計の該当箇所。Invariantsとの衝突やAPI/DB/concurrency判断が必要な場合だけ読む。到達理由をSpec DeviationsまたはImplementation Recordへ残す。
- L3: L2でも仕様が一意でない、設計変更・scope拡大・production/secret/GitHub管理設定操作が必要な場合。自律判断せず停止してClaude architectまたは人間へ戻す。

MANIFESTとClaude生成コードは正しさの基準ではない。矛盾時はAcceptance Criteria、Invariants、実リポジトリを優先する。

## Statusと変更主体

語彙は`DRAFT / APPROVED / IMPLEMENTED / READY / DONE / BLOCKED`だけを使う。

```text
DRAFT --Human only--> APPROVED
APPROVED --Codex only--> IMPLEMENTED
IMPLEMENTED --Claude only--> READY
READY --Human only--> DONE
```

- DRAFT: Claudeが仕様を生成した状態
- APPROVED: Human Gate通過済み。Codexが自律実装してよい
- IMPLEMENTED: Required Testsと`make verify`が成功し、Codex実装が完了
- READY: Claude review通過済み
- DONE: Humanがcommit/pushを完了
- BLOCKED: 設計判断、人間操作、安全境界により停止

Claude/CodexはHuman専用遷移を行わない。Claude reviewのFIXではstatusをIMPLEMENTEDのままCodexへ戻す。設計問題はBLOCKEDにできる。

## Human Gateと成果物

Humanが承認する対象は、Goal / Scope / Out of Scope、Invariants、Acceptance Criteria、MANIFESTの変更範囲だけ。Claudeは`docs/task-ID-*.md`、`docs/commits/task-ID.txt`、一時`MANIFEST.md`を作る。`implementation/`は条件を満たす場合だけの未検証候補で、MANIFESTとともに原則commitしない。

Claudeが`implementation/`を作ってよいのは、新規ファイル中心、既存参照2〜3箇所以内、必要なsignatureが提示済み、自己完結、結合が弱い場合だけ。既存service/repository/handler/state、concurrency、advisory lock、複数module変更では作らずCodexが実リポジトリへ直接実装する。

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

Invariants、Acceptance Criteria、Out of Scope、または実リポジトリの設計前提と衝突する場合、**コードで勝手に解決しない**。

1. L2まで必要な箇所を確認する。
2. 単純な名前・型・signature差分で、対応する概念が実在する場合だけ実リポジトリへ適応する。
3. 設計前提の不一致ならtask docの`Spec Deviations`へ「何を変える必要があるか」「なぜか」を記録する。
4. その経路の実装を止め、statusを`BLOCKED`にする。
5. 次の短い形式でHuman/Claudeへ返す。

```text
BLOCKED: <理由>
Attempted:
- <試した内容>
Conflict:
- <Invariants / AC / scopeとの衝突>
Required decision:
- <必要な判断>
```

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
- `cargo sqlx prepare`、`cargo sqlx migrate`、`sqlx migrate`、`psql`を直接実行しない。
- `DATABASE_URL=...`をコマンドラインや一時環境変数で上書きしてDBコマンドを実行しない。
- DB操作はMakefile → local DB guard → SQLxの経路だけを使う。
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

`make verify` / `make sqlx-prepare` はホストを検査し、非ローカルDBを拒否する。

許可接続先はlocalhost、127.0.0.1、::1、または明示されたlocal Docker DBだけ。安全性を判断できなければ実行しない。

`.env` の秘密情報を読んで回避してはならない。

## 最終品質ゲート

最終検証は必ず次の1コマンド。

```bash
make verify
```

狭いテストは実装中に実行してよいが、最終検証を置き換えない。

`make verify` が実際に成功していないのに「検証済み」「green」と書かない。

成功ログはcommand、exit code、PASS、必要最小限の最終行だけを残す。失敗時だけエラー本体と原因周辺を展開する。依存脆弱性検査は`make audit`として分離し、`make verify`の代用にしない。

## 実装完了時

Status: APPROVEDのtaskだけを実装する。compile/lint/test失敗は通常の停止理由ではなく、scope内で修正可能なら自律修正する。同一原因への修正を3回試しても解決しなければBLOCKEDとする。

実装完了条件は、approved task doc準拠、Required Tests存在、Invariants維持、`make verify` PASS、scope外差分なし、Implementation Record更新済み。すべて満たした場合だけstatusを`IMPLEMENTED`にする。

Implementation RecordはChanged / Decision / Impact / Verify / Remaining形式で短く更新する。

Acceptance Criteriaは`Codex`セクションのうち実際に満たした項目だけ`[x]`にする。`Human — Immediate`と`Human — Deferred`をAIが`[x]`にしない。

Claude の `Review Record` や READY 判定は書き換えない。

## 自律修正の禁止

- 既存テスト削除、`#[ignore]`、assert削除・緩和、Required Tests変更、failure握り潰し
- Invariants、AC、API/DB設計、locking/retry/timeout/security semanticsの変更
- Out of Scope、別task、dependency更新、将来用抽象化、layer再設計

これらが必要になった時点でBLOCKEDとする。

## Git操作

`git status`、`git diff`、`git log`、`git show`などのread-only操作は行ってよい。通常フローでは次を行わない。

- `git commit`
- `git commit --amend`
- `git rebase`
- `git push`
- `git reset --hard`
- `git clean -fd`
- `git tag`
- release
- merge
- deploy

READY後のコミットは人間が行う。
