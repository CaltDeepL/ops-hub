以下を `docs/task-21-dependabot.md` の更新版として使えます。導入後に実際の Dependabot PR が生成されたこと、TypeScript 7 の peer dependency conflict、`package-lock.json` の競合対応まで追加しています。

````markdown
# Task 21: Dependabot を導入

## 1. 背景

Task 20 までで CI の `verify` job が実際の品質ゲートとして機能する状態になったため、依存関係更新の継続的な検知を目的として Dependabot を導入した。

対象リポジトリには以下の3 ecosystem が存在する。

- frontend: npm
- backend: Cargo
- CI / security workflow: GitHub Actions

依存更新を手作業だけに依存せず、GitHub 上で Pull Request として可視化し、既存 CI を通して互換性を確認できる状態を作る。

---

## 2. 着手時点の前提確認

Task 17〜20 は既に main へマージ済みであり、Task 21 はその時点の最新 main を起点とした。

Task 20 後の構成では、`Makefile` の `verify` target は Rust バックエンドの検証に絞られている。

概ね以下を `back_cargo/` で実行する。

```text
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo sqlx prepare --check -- --all-targets --all-features
cargo test --all-targets --all-features
````

また `.github/workflows/ci.yml` では、SQLx の compile-time query validation 前に CI 用 PostgreSQL へ migration を適用する。

```yaml
- name: Run database migrations
  working-directory: back_cargo
  run: cargo sqlx migrate run
```

`#[sqlx::test]` による test DB への migration 適用とは別に、CI の実 DB schema を準備するために必要な処理である。

---

## 3. 実施したこと

`.github/dependabot.yml` を新規作成し、以下の3 ecosystem を対象とした。

| ecosystem      | directory     | 対象                                              |
| -------------- | ------------- | ----------------------------------------------- |
| npm            | `/`           | frontend (`package.json` / `package-lock.json`) |
| cargo          | `/back_cargo` | Rust backend (`Cargo.toml` / `Cargo.lock`)      |
| github-actions | `/`           | `.github/workflows/*.yml` の `uses:`             |

### npm

frontend の manifest は repository root にあるため、

```text
directory: /
```

とした。

### Cargo

Rust crate は、

```text
back_cargo/
```

配下に存在するため、

```text
directory: /back_cargo
```

とした。

`directory: /` では Cargo manifest の位置と一致しない。

### GitHub Actions

`ci.yml` / `security-audit.yml` の reusable Actions は full commit SHA で固定している。

例:

```yaml
uses: actions/checkout@<full-commit-sha> # v6
```

GitHub Actions ecosystem も Dependabot 対象とし、SHA pin を維持しながら更新 PR を生成できる構成とした。

---

## 4. Dependabot の基本方針

各 ecosystem で、

```yaml
open-pull-requests-limit: 5
```

を設定した。

commit message prefix は repository の Conventional Commits 運用に合わせた。

```text
npm             -> chore
cargo           -> chore
github-actions  -> ci
```

初回導入では以下の複雑な制御は入れていない。

* dependency grouping の細かな調整
* schedule.day の固定
* major / minor / patch ごとの細かな policy
* reviewer / assignee の自動指定
* ecosystem 固有の大量の ignore rule

まず実際の Dependabot PR と CI の動作を確認し、必要になったものだけ追加する方針とした。

---

## 5. 設定ファイルの検証

### 5.1 YAML parser の問題

当初、以下で `.github/dependabot.yml` の構文確認を試みた。

```bash
python3 -c "import yaml; yaml.safe_load(open('.github/dependabot.yml'))"
```

しかしローカルの Homebrew 管理 Python には PyYAML が入っておらず、

```text
ModuleNotFoundError: No module named 'yaml'
```

となった。

さらに、

```bash
pip3 install pyyaml
```

は PEP 668 により拒否された。

```text
error: externally-managed-environment
```

system Python を、

```text
--break-system-packages
```

で変更する必要はないため、この回避策は採用しなかった。

YAML syntax 自体は別途 parser を利用できる環境で確認した。

---

## 6. Dependabot の実動作確認

Task 21 完了後、Dependabot が実際に npm dependency update PR を生成することを確認した。

これにより少なくとも以下は実環境で確認できた。

* `.github/dependabot.yml` が GitHub に受理されている
* npm ecosystem が認識されている
* repository root の `package.json` / `package-lock.json` が更新対象になっている
* Dependabot PR に対して通常の CI が起動する

当初の「merge 後に実地確認が必要」という状態から、npm については実動作確認済みとなった。

---

## 7. TypeScript 7 更新 PR で検出した非互換

Dependabot が TypeScript を、

```text
6.0.3 -> 7.0.2
```

へ更新する PR を生成した。

しかし CI の `npm ci` が以下で失敗した。

```text
npm error ERESOLVE could not resolve

Found:
typescript@7.0.2

Could not resolve dependency:

peer typescript@">=4.8.4 <6.1.0"
from typescript-eslint@8.70.0
```

現在の dependency contract は以下。

```text
typescript-eslint 8.70.0
        |
        +-- TypeScript >=4.8.4 <6.1.0

TypeScript 6.0.3
        -> compatible

TypeScript 7.0.2
        -> incompatible
```

したがって、この TypeScript 7 更新は現時点では採用しない。

`npm ci` の失敗は CI の不具合ではなく、互換性のない dependency update を main に入れる前に正しく検出した結果である。

以下による強制解決は行わない。

```text
npm install --force
npm install --legacy-peer-deps
```

peer dependency contract を無視して CI を通すことになるためである。

TypeScript 7 は `typescript-eslint` が正式に対応してから再検討する。

---

## 8. Dependabot 導入後に発生した package-lock.json 競合

Dependabot 導入後、複数の dependency update と main 側の更新が並行したことで `package-lock.json` に merge conflict が発生した。

例として React 系では、

```text
branch:
react              19.3.0
react-dom          19.2.x

main:
react              19.2.x
react-dom          19.3.0
```

のように、別々の dependency update が同じ lockfile を変更していた。

最終的な dependency graph では、

```text
react             19.3.0
react-dom         19.3.0
@types/react      19.3.0
@types/react-dom  19.3.0
```

の組み合わせが整合する状態となった。

この種の競合では `package-lock.json` 内の多数の conflict marker を1件ずつ手編集しない。

原則として、

1. `package.json` の採用 version を決める
2. その dependency contract を正本とする
3. `npm install` で `package-lock.json` を再生成する
4. `npm ci`
5. `npm run lint`
6. `npm run build`

の順で検証する。

CI setup 等、npm dependency 更新を目的としない branch が古い lockfile と競合した場合は、最新 main の lockfile を採用する方針とした。

例:

```bash
git restore --source=origin/main -- package-lock.json
git add package-lock.json
```

その後、

```bash
npm ci
npm run lint
npm run build
```

で検証する。

---

## 9. TypeScript major update の扱い

現時点では、

```text
typescript-eslint 8.70.0
```

が TypeScript 7 を受け付けないため、TypeScript 7 の Dependabot PR はマージ対象外とする。

Dependabot が同じ major update を繰り返し生成する場合は、npm 設定に TypeScript の semver-major ignore を追加することを検討する。

例:

```yaml
ignore:
  - dependency-name: "typescript"
    update-types:
      - "version-update:semver-major"
```

これを設定した場合でも、

```text
6.x -> 6.x
```

の minor / patch update は引き続き追跡できる。

ただし、この ignore は実際に `.github/dependabot.yml` へ追加した時点で Task 文書上も「適用済み」とする。

現時点で未適用なら運用方針としてのみ記録する。

---

## 10. 設計上の判断

### Dependabot PR も通常 CI を必ず通す

Dependabot の更新内容を信頼して直接 merge するのではなく、

```text
Dependabot
    |
    v
dependency update PR
    |
    v
GitHub Actions
    |
    +-- npm ci
    +-- Rust verify
    +-- その他の required check
```

という通常の PR workflow を通す。

今回 TypeScript 7 の非互換を `npm ci` が検出したことで、この構成が実際に機能することを確認できた。

### peer dependency error を無理に回避しない

`ERESOLVE` は「npm が邪魔をしている」のではなく dependency contract の不整合を示している。

したがって、

```text
--force
--legacy-peer-deps
```

で隠さず、依存 package 側の正式サポートを待つ。

### lockfile は生成物として扱う

`package-lock.json` の dependency tree は人手で整合性を維持する対象ではない。

`package.json` を正しく解決した後に npm に再生成させる。

---

## 11. 検証結果

Task 21 では以下を確認した。

* [x] `.github/dependabot.yml` を追加
* [x] npm を `/` で監視
* [x] Cargo を `/back_cargo` で監視
* [x] GitHub Actions を `/` で監視
* [x] `open-pull-requests-limit: 5`
* [x] Conventional Commits に合わせた prefix を設定
* [x] repository layout と directory 指定が一致
* [x] YAML syntax を確認
* [x] GitHub が Dependabot 設定を受理
* [x] npm dependency update PR が実際に生成された
* [x] Dependabot PR に対して CI が起動
* [x] `npm ci` が TypeScript 7 / typescript-eslint 8.70.0 の非互換を検出
* [x] peer dependency conflict を `--force` / `--legacy-peer-deps` で回避しない方針を確認
* [x] `package-lock.json` conflict の解消方針を整理

Task 21 完了。

---

## 12. 未確認の運用項目

npm ecosystem の実動作は確認済み。

以下については、それぞれ実際の更新対象が発生した時点で確認する。

* Cargo Dependabot PR の生成
* GitHub Actions Dependabot PR の生成
* full SHA pin された Actions の更新
* commit-message prefix が各 ecosystem で期待どおりになること

これらは Task 21 の設定導入自体を未完了にするものではなく、継続運用上の確認項目とする。

---

## 13. 次タスクへの引き継ぎ

### main-protection Ruleset

GitHub 上の live Ruleset と、

```text
scripts/github/main-ruleset.json
```

の期待値に差分が残っている。

対象:

```text
strict_required_status_checks_policy
required_review_thread_resolution
require_extra_approval_for_unattributed_changes
```

repository 内の期待値を正本として live Ruleset を一致させる作業が必要。

### 未使用 scripts の整理

現在の簡略化された `Makefile` では以下が呼ばれていない。

```text
scripts/assert_local_database_url.py
scripts/check_ruleset_contract.py
scripts/run_quiet.py
```

今後、

* 再び品質ゲートから利用する
* 別 verification command として残す
* 不要なら削除する

のどれを正とするか整理する。

### TypeScript 7

現時点では、

```text
typescript-eslint 8.70.0
requires TypeScript < 6.1.0
```

のため TypeScript 7 は保留。

`typescript-eslint` の正式対応後に再度 Dependabot update を許可する。

---




