# ops-hub

複数の個人プロジェクトを横断監視し、異常をSlackへ通知する外形監視・通知ハブです。

現在のリポジトリは、完成した監視サービスではなく再構築途中の状態です。設計・task文書と実装ファイルを区別するため、以下には2026-09-11にローカル作業ツリーとGitHub `main`を照合した結果を記載しています。

## 現在の実装状態

| 項目 | 状態 | 確認結果 |
|---|---|---|
| フロントエンド | 不完全なscaffold | React 19 / Vite 8の初期画面は存在するがbuild不能 |
| Rustバックエンド | 未配置 | `Cargo.toml`、`back_cargo/`、Rust sourceが存在しない |
| PostgreSQL / migration | 未配置 | migrationと実行用composeが存在しない |
| HTTP API | 未実装 | `/health`や`POST /v1/runs`を提供する実コードは存在しない |
| Task 01〜06文書 | 存在 | 設計・作業記録であり、現treeの実装完了を意味しない |
| Dependabot | 未導入 | `.github/dependabot.yml`が存在しない |
| CI | 要修正 | workflowは存在するが、検証コマンドを実行していない |
| Security Audit | 要修正 | workflowは存在するが、存在しない`back_cargo`を参照する |
| main保護 | 有効・要調整 | Repository Rulesetはactiveだが、期待設定と差分あり |

GitHub `main`の先頭は`511fce6`、確認時のローカル作業ブランチは`infrastructure/ci-setup-fix`（`6582a07`）です。GitHub `main`にもRustバックエンド、Makefile、compose、Dependabot設定はありません。

## 現在確認できるもの

フロントエンドsourceと依存定義はありますが、追跡対象に`vite.config.ts`がなく、`tsconfig.node.json`がこの欠落fileだけを入力としているため、現時点のbuildは失敗します。

```bash
npm ci
npm run dev
```

`npm run lint`は成功します。`npm run build`は次の既知エラーで失敗します。

```text
TS18003: No inputs were found in tsconfig.node.json (include: vite.config.ts)
```

`npm run dev`を利用する場合、Viteが表示するURLを使用してください。バックエンドやDBの起動手順は、実装ファイルが復元されるまで利用できません。

## GitHub Actions

### CI

`.github/workflows/ci.yml`には`verify` jobがありますが、現在実行するのは次だけです。

- checkout
- Rust cache初期化
- Node.js setup
- `npm ci`
- `sqlx-cli` install

`npm run lint`、`npm run build`、Rust compile/test、migration、`make verify`は実行していません。直近で確認したrun #12はgreenですが、上記setupが成功したことだけを示し、アプリ品質の合格を意味しません。

### Security Audit

`.github/workflows/security-audit.yml`は毎週月曜と手動実行に設定されています。ただし`working-directory: back_cargo`を参照しており、現在そのdirectoryがないため、有効な監査としては扱えません。

### Dependabot

未導入です。`.github/dependabot.yml`はローカルにもGitHub `main`にもありません。

## Repository Ruleset

GitHubには`main-protection` Ruleset（ID `22847969`）が存在し、`enforcement: active`です。

現在有効な保護:

- default branchの削除禁止
- non-fast-forward更新禁止
- pull request経由を要求
- required status checkとして`verify`を要求

ただし、リポジトリ内の期待設定`main-ruleset.json`とは次が一致していません。

| 設定 | GitHub live | リポジトリ内の期待値 |
|---|---:|---:|
| strict required status checks | `true` | `false` |
| review thread resolution必須 | `true` | `false` |
| unattributed changesへの追加approval | `true` | `false` |

さらに、required checkの`verify`自体が実質的な検証を行っていないため、Rulesetは存在していても品質ゲートとしては未完成です。`bypass_actors`は無認証APIでは取得できないため、空であることは認証済みGitHub画面/APIで人間が確認する必要があります。

- [Ruleset設定](https://github.com/CaltDeepL/ops-hub/rules/22847969)
- [GitHub Actions](https://github.com/CaltDeepL/ops-hub/actions)

## 現在のディレクトリ構成

```text
.
├── .github/workflows/
│   ├── ci.yml
│   └── security-audit.yml
├── docs/
│   ├── Task_01_skeleton.md
│   ├── ...
│   └── Task_06 post runs.md
├── scripts/
│   ├── check_ruleset_contract.py
│   └── github/verify_main_ruleset.py
├── src/                    # React/Vite scaffold
├── package.json
└── README.md
```

## 設計上の目標

最終的には、Render / Neonの無料枠を前提に、常駐schedulerを持たずGitHub Actionsから`POST /v1/runs`を定期実行する構成を目指します。想定スタックはRust / Axum / SQLx / PostgreSQL / Reactです。

この節は将来構想であり、現在利用可能な機能を示すものではありません。Task文書内のコード例も未検証案を含むため、実ファイルとtest結果を正本として扱ってください。

## 復旧・導入の優先順

1. `vite.config.ts`を復元し、frontend buildをgreenに戻す。
2. バックエンド、migration、compose、品質ゲートを実ファイルとして復元する。
3. CIから実際のlint/build/compile/test/validationを実行する。
4. Rulesetのlive設定を期待値へ合わせ、failure/success pathを実証する。
5. 実在するpackage ecosystemに合わせてDependabotを追加する。

現状確認コマンド:

```bash
git status --short --branch
git ls-files
python3 scripts/github/verify_main_ruleset.py
```
