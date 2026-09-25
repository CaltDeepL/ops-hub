# Task 21: Dependabot を導入

| 項目 | 内容 |
|---|---|
| 目的 | npm / Cargo / GitHub Actions の依存更新を PR として可視化し、既存 CI で互換性を確認できる状態を作る |
| 完了条件 | `.github/dependabot.yml` が GitHub に受理され、実際に更新 PR が生成されて CI が起動する |
| 前提 | Task 17〜20 が main にマージ済み |
| ステータス | 完了 |

## 1. 実施内容

`.github/dependabot.yml` を新規作成し、3 ecosystem を週次で監視する。

| ecosystem | directory | 対象 |
|---|---|---|
| npm | `/` | frontend（`package.json` / `package-lock.json`） |
| cargo | `/back_cargo` | Rust backend（`Cargo.toml` / `Cargo.lock`） |
| github-actions | `/` | `.github/workflows/*.yml` の `uses:` |

- 各 ecosystem で `open-pull-requests-limit: 5`
- commit message prefix は Conventional Commits に合わせ、npm / cargo → `chore`、github-actions → `ci`
- npm の TypeScript semver-major update を `ignore` で除外（7章の非互換対応。適用済み）

```yaml
ignore:
  - dependency-name: "typescript"
    update-types:
      - "version-update:semver-major"
```

### 実動作の確認

- `.github/dependabot.yml` が GitHub に受理された
- npm / Cargo（例：`tower-http 0.6.11 → 0.7.1`）/ GitHub Actions（例：`actions/setup-node 6.5.0 → 7.0.0`）の各 ecosystem で更新 PR が実際に生成され、main にマージされた
- Dependabot PR に対して通常の CI が起動する

### 検証結果

- [x] `.github/dependabot.yml` を追加
- [x] npm を `/`、Cargo を `/back_cargo`、GitHub Actions を `/` で監視
- [x] `open-pull-requests-limit: 5`
- [x] Conventional Commits に合わせた prefix を設定
- [x] repository layout と directory 指定が一致
- [x] YAML syntax を確認
- [x] GitHub が Dependabot 設定を受理
- [x] 更新 PR が実際に生成され、CI が起動
- [x] `npm ci` が TypeScript 7 / typescript-eslint 8.70.0 の非互換を検出
- [x] peer dependency conflict を `--force` / `--legacy-peer-deps` で回避しない方針を確認
- [x] `package-lock.json` conflict の解消方針を整理

## 2. 設計判断

### directory は manifest の実位置に合わせる

frontend の manifest は repository root にあるため npm は `/`。Rust crate は `back_cargo/` 配下なので Cargo は `/back_cargo`（`/` では manifest の位置と一致しない）。

### GitHub Actions も対象にする

`ci.yml` / `security-audit.yml` の Actions は full commit SHA で固定している（`uses: actions/checkout@<full-commit-sha> # v6`）。Dependabot の更新 PR で SHA とバージョンコメントを一緒に上げられるようにする。

### 初回導入は最小構成

dependency grouping、`schedule.day` の固定、major / minor / patch ごとの policy、reviewer / assignee の自動指定、大量の ignore rule は入れていない。実際の PR と CI の動作を見て、必要になったものだけ追加する。

### Dependabot PR も通常 CI を必ず通す

```text
Dependabot → dependency update PR → GitHub Actions（npm ci / Rust verify / required check）
```

TypeScript 7 の非互換を `npm ci` が検出したことで、この構成が実際に機能することを確認できた。

### peer dependency error を無理に回避しない

`ERESOLVE` は dependency contract の不整合を示している。`--force` / `--legacy-peer-deps` で隠さず、依存 package 側の正式サポートを待つ。

### lockfile は生成物として扱う

`package-lock.json` の dependency tree は人手で整合性を維持する対象ではない。`package.json` を正しく解決した後に npm に再生成させる。

## 3. つまずいた点と教訓

### #1 ローカルで YAML を検証できなかった

```bash
python3 -c "import yaml; yaml.safe_load(open('.github/dependabot.yml'))"
# ModuleNotFoundError: No module named 'yaml'

pip3 install pyyaml
# error: externally-managed-environment（PEP 668）
```

system Python を `--break-system-packages` で変更する必要はないため採用せず、parser を利用できる別環境で構文確認した。

### #2 TypeScript 7 更新 PR で `npm ci` が失敗

Dependabot が TypeScript を `6.0.3 → 7.0.2` へ更新する PR を生成したが、CI の `npm ci` が失敗した。

```text
npm error ERESOLVE could not resolve
Found: typescript@7.0.2
Could not resolve dependency:
peer typescript@">=4.8.4 <6.1.0" from typescript-eslint@8.70.0
```

CI の不具合ではなく、互換性のない update を main に入れる前に正しく検出した結果。TypeScript 7 は `typescript-eslint` が正式対応するまで保留し、semver-major を `ignore` に追加した（6.x の minor / patch は引き続き追跡される）。

### #3 複数の更新が並行して `package-lock.json` が競合

React 系では branch 側が `react 19.3.0 / react-dom 19.2.x`、main 側が `react 19.2.x / react-dom 19.3.0` のように、別々の update が同じ lockfile を変更していた。最終的には `react` / `react-dom` / `@types/react` / `@types/react-dom` をすべて `19.3.0` に揃えて整合した。

**教訓：lockfile の conflict marker を1件ずつ手編集しない。** 原則は以下。

1. `package.json` の採用 version を決める
2. その dependency contract を正本とする
3. `npm install` で `package-lock.json` を再生成する
4. `npm ci` → `npm run lint` → `npm run build` で検証する

npm 依存更新を目的としない branch（CI setup 等）が古い lockfile と競合した場合は、最新 main の lockfile を採用する。

## 4. 再現コマンド

```bash
# 依存更新を目的としない branch の lockfile 競合を解消する
git restore --source=origin/main -- package-lock.json
git add package-lock.json

# 検証
npm ci
npm run lint
npm run build
```

## 5. 次タスクへの引き継ぎ

- **main-protection Ruleset**：GitHub 上の live Ruleset と `scripts/github/main-ruleset.json` に差分が残っている（`strict_required_status_checks_policy` / `required_review_thread_resolution` / `require_extra_approval_for_unattributed_changes`）。一致させる作業が必要
- **未使用 scripts の整理**：簡略化された `Makefile` から `scripts/assert_local_database_url.py` / `scripts/check_ruleset_contract.py` / `scripts/run_quiet.py` が呼ばれていない。品質ゲートに戻す・別コマンドとして残す・削除する、のどれを正とするか整理する
- **TypeScript 7**：`typescript-eslint 8.70.0` が TypeScript < 6.1.0 を要求するため保留。正式対応後に `ignore` を外して再検討する
- 継続運用の確認項目：各 ecosystem の commit-message prefix が期待どおりになっていること
