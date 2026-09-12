# ops-hub

[![CI](https://github.com/CaltDeepL/ops-hub/actions/workflows/ci.yml/badge.svg)](https://github.com/CaltDeepL/ops-hub/actions/workflows/ci.yml)

**Rust + PostgreSQL で実装している、複数サービス向けの外形監視・通知ハブ。**

複数の個人プロジェクトを定期的に監視し、HTTP probe の結果を記録して、異常時に incident を生成し Slack へ通知することを目的としています。

Render / Neon の無料枠を前提に、常駐 scheduler は持たず、外部 scheduler から `POST /v1/runs` を呼び出して1巡を開始する構成です。

> **ポートフォリオプロジェクトです。** 現在は監視実行の入口・排他制御・取り残し回収・CI / Ruleset まで実装済みで、実際の target probe は Task 08 以降で実装します。

---

## なぜ作ったか

複数の個人プロジェクトを運用していると、「サービスが落ちていないか」をそれぞれ個別に確認する必要があります。

このプロジェクトでは、単に HTTP を定期実行するだけではなく、特に次の点を自分で設計・実装することを目的としています。

**同じ巡回を二重実行させないこと。**  
GitHub Actions などの外部 scheduler から呼び出す場合、前回の巡回が終わる前に次の実行が始まる可能性があります。PostgreSQL advisory lock を利用し、同時に1巡だけが動くようにしています。

**途中でプロセスが死んでも状態を回収できること。**  
実行途中でプロセスが消えると、DB には `running` の run が残ります。一方、advisory lock はセッション単位なので、異常なセッションが残れば以降の巡回を塞ぐ可能性があります。この2種類を分け、自動スイーパーと手動の脱出ハッチを用意しています。

**無料枠でも運用できること。**  
常駐 scheduler をアプリ内に持つと、Render のスピンダウンや無料枠の制約と相性がよくありません。定期実行そのものは外部に任せ、アプリは1回分の巡回を開始する API を提供する設計にしています。

---

## Features

| 機能 | エンドポイント / CLI |
|---|---|
| DB を含む health check | `GET /health` |
| process liveness | `GET /livez` |
| 監視 run の開始 | `POST /v1/runs` |
| stale run の自動回収 | run 開始時の sweeper |
| advisory lock holder の確認 | `ops-hub unlock` |
| advisory lock holder の手動切断 | `ops-hub unlock --force` |

`POST /v1/runs` は advisory lock を取得できた場合に `202 Accepted` を返し、バックグラウンドで run を実行します。

すでに別の run が実行中の場合はエラーにはせず、

```text
200 already_running
```

として扱います。

競合時の `run_id` はタイミングによって取得できず、`null` になる場合があります。

現在の `run_service::perform` はまだ target を巡回せず、

```text
targets_checked = 0
```

で完了します。

target 取得、HTTP probe、状態遷移、incident、Slack 通知は今後の実装範囲です。

---

## Architecture

```text
        GitHub Actions / external scheduler
                    │
                    │ POST /v1/runs
                    ▼
             Rust + axum API
                    │
        ┌───────────┴───────────┐
        │                       │
        ▼                       ▼
 stale run sweeper       advisory lock
        │                       │
        └───────────┬───────────┘
                    │
                    ▼
               run_service
                    │
                    │ Task 08以降
                    ▼
             target / probe
                    │
                    ▼
              checks / incidents
                    │
                    ▼
             outbox / Slack
                    │
                    ▼
              PostgreSQL 17
                （Neon）
```

現在は `run_service` の入口、排他制御、run の記録、stale run の回収まで実装済みです。

### ディレクトリ構成

```text
ops-hub/
├── .github/
│   ├── dependabot.yml
│   └── workflows/
│       ├── ci.yml
│       └── security-audit.yml
├── back_cargo/
│   ├── .sqlx/               # SQLx offline query metadata
│   ├── migrations/          # 0001〜0004
│   ├── src/
│   │   ├── main.rs          # serve / healthcheck / unlock
│   │   ├── lib.rs           # router
│   │   ├── config.rs
│   │   ├── recovery.rs      # stale run / advisory lock recovery
│   │   └── service/
│   │       └── run_service.rs
│   ├── tests/
│   │   ├── runs_test.rs
│   │   ├── run_lock_test.rs
│   │   └── recovery_test.rs
│   ├── Cargo.toml
│   └── compose.yaml
├── src/                     # React / Vite scaffold
├── scripts/
│   ├── assert_local_database_url.py
│   ├── check_ruleset_contract.py
│   └── github/
│       ├── main-ruleset.json
│       ├── verify_main_ruleset.py
│       └── sync_main_ruleset.py
├── Makefile
└── package.json
```

---

## Key Design Decisions

### なぜ scheduler をアプリ内に持たないのか

Render の無料枠ではアクセスがない状態でプロセスが停止するため、アプリ内の timer や常駐 scheduler を実行基盤として信用できません。

そのため、

```text
external scheduler
        ↓
POST /v1/runs
        ↓
1回分の巡回
```

という構成にしています。

定期実行の責務と監視処理そのものを分離することで、アプリが再起動しても次回の外部呼び出しから処理を再開できます。

### なぜ advisory lock を使うのか

同じ監視 run が二重に動くと、check・incident・notification が重複する可能性があります。

PostgreSQL の session-level advisory lock を利用し、

```text
lock acquired
    ↓
run start
    ↓
background work
    ↓
run finish
    ↓
lock release
```

の間、1つの run だけを許可します。

競合は障害ではなく「すでに実行中」という正常系として扱います。

### なぜ stale run と stale lock を別々に回収するのか

2つは性質が異なります。

| 対象 | 排他への影響 | 回収 |
|---|---|---|
| DB に残った `running` run | なし | 自動 |
| 生存セッションが保持する advisory lock | あり | 手動 |

プロセス終了時には advisory lock は通常 PostgreSQL が解放しますが、DB 上の `runs.status` は自動では更新されません。

そのため stale run は sweeper で自動回収します。

一方、advisory lock holder の強制切断は正常実行中のセッションを殺す危険があるため、

```bash
ops-hub unlock
ops-hub unlock --force
```

という明示的な操作に限定しています。

### なぜ stale run の回収を run 開始前に行うのか

常駐 scheduler を持たないため、定期的に cleanup を実行できる場所がありません。

そこで `POST /v1/runs` の開始処理を、

```text
sweep
  ↓
lock
  ↓
run
```

の順にしています。

sweep に失敗しても本来の監視 run は続行します。

stale run の整理は観測情報の修復であり、排他制御そのものではないためです。

### なぜ `pg_locks` の問い合わせだけ runtime query なのか

通常のアプリケーション query は SQLx macro と `.sqlx` offline metadata で compile-time validation しています。

一方 `pg_locks` / `pg_stat_activity` は PostgreSQL の system catalog であり、server version による列定義の違いをローカル `.sqlx` metadata に固定したくありません。

そのため recovery 用の system catalog query だけは `sqlx::query` を利用しています。

### なぜ `make verify` で remote DB を拒否するのか

SQLx の compile-time validation は `DATABASE_URL` の database schema を参照します。

ローカル shell に Neon など本番 DB の URL が残っている状態で品質検証を実行しないよう、

```text
localhost
127.0.0.1
db
postgres
```

以外の host を `scripts/assert_local_database_url.py` で拒否しています。

---

## 技術スタック

| 領域 | 技術 |
|---|---|
| Backend | Rust 1.96 / axum 0.8 / Tokio |
| Database | PostgreSQL 17 |
| DB access | SQLx 0.9 |
| Frontend | React 19 / TypeScript / Vite 8 |
| Local runtime | Docker / Docker Compose |
| CI | GitHub Actions |
| Dependency updates | Dependabot |
| Production target | Render / Neon |

---

## Setup

### 必要なもの

- Rust 1.96 以上
- Node.js 24
- Docker / Docker Compose
- `sqlx-cli` 0.9

### Backend / PostgreSQL

repository root から起動します。

```bash
docker compose -f back_cargo/compose.yaml up --build -d
```

既定の host port:

| 対象 | Host | Container |
|---|---:|---:|
| API | `8081` | `8080` |
| PostgreSQL | `5433` | `5432` |

疎通確認:

```bash
curl -f http://localhost:8081/health
curl -f http://localhost:8081/livez
```

run の開始:

```bash
curl -i -X POST http://localhost:8081/v1/runs
```

### Frontend

`package.json` は repository root にあります。

```bash
npm ci
npm run dev
```

個別検証:

```bash
npm run lint
npm run build
```

---

## Testing

ローカル PostgreSQL を起動し、migration を適用します。

```bash
cd back_cargo
docker compose up -d
cargo sqlx migrate run
cd ..
```

SQLx offline metadata を更新する場合:

```bash
make sqlx-prepare
```

品質ゲート:

```bash
DATABASE_URL=postgres://ops_hub:ops_hub@localhost:5433/ops_hub make verify
```

`make verify` は以下を実行します。

```text
DATABASE_URL safety guard
        ↓
Ruleset / CI contract check
        ↓
cargo fmt --check
        ↓
cargo clippy -D warnings
        ↓
cargo sqlx prepare --check
        ↓
cargo test
```

Task 07 完了時点の Rust test:

| 分類 | 件数 |
|---|---:|
| unit tests | 29 |
| recovery integration tests | 11 |
| advisory lock tests | 4 |
| run API tests | 5 |
| **合計** | **49** |

```text
49 passed
0 failed
```

recovery tests では、stale run の回収、閾値判定、複数 run の一括処理、advisory lock holder の取得、session terminate による lock 解放などを実 DB で検証しています。

---

## CI / Repository Protection

```text
Pull Request
     ↓
GitHub Actions
     ↓
PostgreSQL 17
     ↓
migration
     ↓
make verify
     ↓
required check: verify
     ↓
main-protection Ruleset
     ↓
merge
```

### CI

[`ci.yml`](https://github.com/CaltDeepL/ops-hub/blob/main/.github/workflows/ci.yml) は `main` への push と pull request で実行します。

主な処理:

1. PostgreSQL 17 service を起動
2. `npm ci`
3. `sqlx-cli` を install
4. migration を適用
5. `make verify`

現在 frontend の、

```bash
npm run lint
npm run build
```

は `make verify` には含まれておらず、別途実行します。

### Dependabot

[`dependabot.yml`](https://github.com/CaltDeepL/ops-hub/blob/main/.github/dependabot.yml) で以下を週次監視しています。

| ecosystem | directory |
|---|---|
| npm | `/` |
| Cargo | `/back_cargo` |
| GitHub Actions | `/` |

Dependabot が実際に update PR を生成することまで確認済みです。

TypeScript の semver-major update は、現行 toolchain との互換性のため除外しています。

### Security Audit

[`security-audit.yml`](https://github.com/CaltDeepL/ops-hub/blob/main/.github/workflows/security-audit.yml) では `cargo audit` を毎週月曜と手動実行で行います。

2026-09-13 時点では workflow は設定済みですが、実行履歴の確認は今後の運用項目です。

### Repository Ruleset

[`main-protection`](https://github.com/CaltDeepL/ops-hub/rules/22847969) Ruleset は `enforcement: active`。

| 項目 | 設定 |
|---|---|
| Default branch deletion | 禁止 |
| Non-fast-forward | 禁止 |
| Pull request | 必須 |
| Required status check | `verify` |
| Strict status checks | 有効 |
| Review thread resolution | 必須 |
| Extra approval for unattributed changes | 必須 |
| Bypass actors | なし |

`scripts/github/main-ruleset.json` を repository 内の正本とし、GitHub live 設定との一致を確認済みです。

Task 22 では実際の PR で、

```text
verify FAIL
    ↓
merge BLOCKED
```

と、

```text
verify PASS
    ↓
merge ALLOWED
```

の両方を確認しています。

---

## Implementation Status

### Core monitoring

| Task | 状態 |
|---|---|
| Task 01〜06 | 完了 |
| Task 07: stale run / advisory lock recovery | 完了 |
| Task 08: target取得・probe・check記録 | 次に実装 |

### Quality / infrastructure

| Task | 状態 |
|---|---|
| Task 17〜19 | 復旧・repository整理 完了 |
| Task 20 | CI の `make verify` 実行を実質化 |
| Task 21 | Dependabot 導入 |
| Task 22 | main Ruleset の正本・live 設定を一致 |

---

## Future Work

現在の run は、

```text
request
   ↓
sweep
   ↓
lock
   ↓
run record
   ↓
perform()
   ↓
targets_checked = 0
```

まで実装されています。

次に監視サービスとして必要なのは以下です。

| 優先度 | 項目 | 完了条件 |
|---|---|---|
| P1 | target取得・HTTP probe | 登録 target を巡回し結果を取得できる |
| P1 | checks の保存 | probe 結果を DB に永続化できる |
| P1 | 状態遷移 | success / failure の変化を判定できる |
| P1 | incident | 障害発生・復旧を記録できる |
| P1 | outbox / Slack | 通知を失わず Slack へ送信できる |
| P2 | 日次集計 | uptime / failure 等を集計できる |
| P2 | status page | 監視状態を UI から確認できる |
| P2 | frontend CI | lint / build を required quality gate に含める |
| P2 | production unlock | Render 上で安全に `unlock` を実行できる運用経路を確立 |

---

## 開発記録

タスクごとの設計判断・失敗・検証結果は `docs/` に記録しています。

このプロジェクトでは、完成したコードだけでなく、

- CI が green でも実際の品質検証をしていなかった
- SQLx compile-time validation の前に CI DB migration が必要だった
- 古い branch との merge で `back_cargo/` が消失した
- Ruleset の live 設定の方が repository 内の正本より安全だった
- stale run と stale advisory lock は別の回収方法が必要だった
- `pg_locks` は test DB 単位ではなく PostgreSQL cluster 全体を参照する

といった、実装中に判明した問題も task document に残しています。

過去の task 文書はその時点のスナップショットを含むため、現在の動作を判断する場合は、

```text
実コード
  ↓
test
  ↓
CI
```

を優先してください。
````
