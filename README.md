# ops-hub

複数の個人プロジェクトを横断監視し、異常をSlackへ通知する外形監視・通知ハブです。

Render / Neonの無料枠を前提に、常駐schedulerを持たず、外部schedulerから`POST /v1/runs`を呼び出して1巡を開始する構成を目指しています。

## 現在の状態

2026-09-13にローカル`main`とGitHub `main`を照合しました。両方ともcommit [`d5d828e`](https://github.com/CaltDeepL/ops-hub/commit/d5d828eec6d6646aabe3b77aff5734c1a8ff1f86)で一致し、確認開始時のworking treeはcleanでした。

| 項目 | 状態 | 現在の範囲 |
|---|---|---|
| Rust backend | 実装済み（Task 01〜07） | HTTP基盤、DB、run起動、advisory lock、stale run回収、手動unlock |
| PostgreSQL | 実装済み | migration 0001〜0004、SQLx offline metadata |
| HTTP API | 一部実装 | `/health`、`/livez`、`POST /v1/runs` |
| 実際のtarget監視 | 未実装 | run内部のprobe処理は現在no-op |
| incident / Slack / outbox | 未実装 | Task 08以降の範囲 |
| frontend | scaffold | React 19 / Vite 8の初期画面。運用画面は未実装 |
| CI | 稼働中 | PostgreSQL migration後に`make verify`を実行 |
| Dependabot | 稼働中 | npm / Cargo / GitHub Actionsを週次監視 |
| Security Audit | 設定済み | Cargo auditを週次・手動実行。確認時点では実行履歴なし |
| main保護 | 稼働中 | activeなRepository Rulesetとrequired check `verify` |

## 実装済みの機能

### HTTP API

| Method | Path | 動作 |
|---|---|---|
| `GET` | `/health` | DB疎通を含むhealth check。DB異常時は503 |
| `GET` | `/livez` | processのliveness確認 |
| `POST` | `/v1/runs` | advisory lockを取得してrunを開始し、202を即時返却 |

`POST /v1/runs`のlock競合はエラーではなく、`200 already_running`として扱います。競合時の`run_id`はraceにより`null`になり得ます。

現在、runのバックグラウンド処理は対象を巡回せず、`targets_checked = 0`で完了します。target取得、HTTP probe、状態遷移、incident、通知は未実装です。

### 取り残し回収

- 一定時間`running`のまま残ったrunを`failed`へ更新
- advisory lockの保持sessionを調べる`unlock` CLI
- `--force`指定時だけ、idle時間の安全条件を満たすsessionを切断
- run完了記録後にlockを解放

## 技術構成

| 分類 | 採用技術 |
|---|---|
| Backend | Rust 1.96 / Axum 0.8 / Tokio |
| Database | PostgreSQL 17 / SQLx 0.9 |
| Frontend | React 19 / TypeScript / Vite 8 |
| Local runtime | Docker Compose |
| CI / dependency updates | GitHub Actions / Dependabot |
| 想定production | Render / Neon |

```text
.
├── back_cargo/
│   ├── migrations/          # 0001〜0004
│   ├── src/                 # Axum / service / repository / recovery
│   ├── tests/               # run lock / runs / recovery integration tests
│   ├── .sqlx/               # offline query metadata
│   ├── Cargo.toml
│   └── compose.yaml
├── src/                     # React/Vite scaffold
├── .github/
│   ├── dependabot.yml
│   └── workflows/
│       ├── ci.yml
│       └── security-audit.yml
├── scripts/                 # DB guard / Ruleset verification・sync
└── Makefile
```

## ローカル開発

必要なもの:

- Rust 1.96以上
- Node.js 24
- Docker / Docker Compose
- `sqlx-cli` 0.9

### Frontend

```bash
npm ci
npm run dev
```

個別検証:

```bash
npm run lint
npm run build
```

### Backend / PostgreSQL

```bash
docker compose -f back_cargo/compose.yaml up --build -d
curl http://localhost:8081/health
curl http://localhost:8081/livez
curl -i -X POST http://localhost:8081/v1/runs
```

既定のhost portはAPI `8081`、PostgreSQL `5433`です。コンテナ内部はAPI `8080`、PostgreSQL `5432`です。

### 品質確認

```bash
make verify
```

`make verify`はlocal PostgreSQLの`DATABASE_URL`が明示されていない場合、安全のため失敗します。現在のtargetはDB guard、Ruleset contract、Rust format、Clippy、SQLx metadata、Rust testです。frontendの`npm run lint`と`npm run build`は別途実行してください。

今回のローカル確認結果:

- `npm run lint`: PASS
- `npm run build`: PASS
- `make verify`: `DATABASE_URL`未設定をguardが検出して停止
- GitHub Actions `verify`: PASS
- live Ruleset verification: PASS

## CI / Dependabot / Repository protection

### CI

[`ci.yml`](https://github.com/CaltDeepL/ops-hub/blob/main/.github/workflows/ci.yml)はpush to `main`とpull requestで実行されます。

1. PostgreSQL 17 serviceを起動
2. frontend dependencyを`npm ci`でinstall
3. `sqlx-cli`をinstall
4. migrationを適用
5. `make verify`を実行

最新mainの[CI run](https://github.com/CaltDeepL/ops-hub/actions/runs/34725093097)は成功しています。ただし、frontend lint/buildは現在`make verify`に含まれず、CIでも未実行です。

### Dependabot

[`dependabot.yml`](https://github.com/CaltDeepL/ops-hub/blob/main/.github/dependabot.yml)は次の3 ecosystemを週次監視します。

- npm: `/`
- Cargo: `/back_cargo`
- GitHub Actions: `/`

Dependabotのdynamic update runと更新PRが実際に生成されることを確認済みです。TypeScriptのsemver-major更新は、現行toolchainとの互換性のため除外しています。

### Security Audit

[`security-audit.yml`](https://github.com/CaltDeepL/ops-hub/blob/main/.github/workflows/security-audit.yml)は毎週月曜と手動実行で`cargo audit`を行います。workflowはactiveですが、2026-09-13の確認時点では実行履歴を確認できていません。

### Repository Ruleset

[`main-protection`](https://github.com/CaltDeepL/ops-hub/rules/22847969) Rulesetは`enforcement: active`です。

- default branchの削除禁止
- non-fast-forward更新禁止
- pull request経由を要求
- required status check `verify`
- latest mainへの追従を要求するstrict check
- review threadの解決を要求
- unattributed changesへの追加approvalを要求

`scripts/github/main-ruleset.json`とGitHub live設定は、read-only verifierで一致を確認済みです。`bypass_actors`は無認証APIでは返らないため、この項目だけは認証済みGitHub画面/APIで確認してください。

## 次に実装する範囲

現在のrun処理は入口と排他・回収までです。プロダクトとして監視を成立させるには、次が必要です。

1. target取得とHTTP probe
2. check結果の保存と状態遷移
3. incident生成・復旧処理
4. outboxとSlack通知
5. 日次集計とstatus page
6. frontend lint/buildのCI必須化

task文書には当時の設計案や作業記録も含まれます。現在の動作判断では、実コード、test、CI結果を優先してください。
