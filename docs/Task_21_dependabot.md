以下を `docs/task-21-dependabot.md` の完成版として使えます。Task 20 後の最新状態、Dependabot 3 ecosystem、PyYAML 検証時のつまずき、未検証事項まで反映しています。

````markdown
# Task 21: Dependabot を導入

## 1. 背景

Task 20 までで CI の `verify` job が実際に Rust バックエンドの品質ゲートを実行する状態になったため、次の運用整備として Dependabot を導入する。

対象リポジトリには以下の3種類の更新対象が存在する。

- frontend: npm
- backend: Cargo
- CI / security workflow: GitHub Actions

依存更新を手動確認だけに依存せず、GitHub Dependabot によって定期的に Pull Request として可視化できる状態を作る。

---

## 2. 着手時点の前提確認

着手前に GitHub / `main` の現状を確認した。

Task 17〜20 は PR #7〜#9 を経由して既に `main` へマージ済みであり、今回の作業は最新 `main` の以下の commit を起点とした。

```text
0f762ce
````

また、Task 20 完了までの過程で当初案から以下の追加修正が行われていた。

### 2.1 Makefile の簡略化

現在の `verify` target は Rust バックエンドの品質ゲートに限定されている。

概ね以下の検証を `back_cargo/` で実行する。

```text
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo sqlx prepare --check -- --all-targets --all-features
cargo test --all-targets --all-features
```

以前の AI scaffold 由来の以下は `make verify` から外れている。

```text
scripts/assert_local_database_url.py
scripts/check_ruleset_contract.py
AI tooling tests
npm lint / build
```

### 2.2 CI で migration を事前適用

`.github/workflows/ci.yml` には `make verify` より前に、

```yaml
- name: Run database migrations
  working-directory: back_cargo
  run: cargo sqlx migrate run
```

が追加されている。

これは必要な修正だった。

`cargo sqlx prepare --check` や `sqlx::query!` / `query_scalar!` の compile-time validation は、`DATABASE_URL` が指す実 database schema を参照する。

GitHub Actions の PostgreSQL service は毎 run 空の DB として起動するため、事前に migration を適用しなければ、

```text
relation "runs" does not exist
```

等で compile-time validation が失敗する。

一方、`#[sqlx::test]` による migration 適用は test ごとに生成される DB に対するものであり、この CI DB 初期化とは別の責務である。

以上を前提として Task 21 を実施した。

---

## 3. 実施したこと

`.github/dependabot.yml` を新規作成した。

対象 ecosystem は3つ。

| ecosystem        | directory     | 対象                                              |
| ---------------- | ------------- | ----------------------------------------------- |
| `npm`            | `/`           | frontend (`package.json` / `package-lock.json`) |
| `cargo`          | `/back_cargo` | Rust backend (`Cargo.toml` / `Cargo.lock`)      |
| `github-actions` | `/`           | `.github/workflows/*.yml` の `uses:`             |

### 3.1 npm

frontend は repository root に存在するため、

```yaml
package-ecosystem: npm
directory: /
```

とした。

### 3.2 Cargo

Rust crate は repository root ではなく、

```text
back_cargo/
```

配下に存在する。

そのため Dependabot の Cargo directory は、

```yaml
package-ecosystem: cargo
directory: /back_cargo
```

とした。

`directory: /` にすると `back_cargo/Cargo.toml` を対象として正しく認識できないため、この repository layout に合わせて明示した。

### 3.3 GitHub Actions

以下の workflow では Actions の version を tag ではなく full commit SHA で固定している。

```text
.github/workflows/ci.yml
.github/workflows/security-audit.yml
```

例:

```yaml
uses: actions/checkout@<full-commit-sha> # v6
```

Dependabot は GitHub Actions ecosystem としてこの形式を認識し、更新可能な Action があれば Pull Request を生成する。

このため、

```yaml
package-ecosystem: github-actions
directory: /
```

も対象に含めた。

---

## 4. Dependabot の更新方針

初回導入では複雑な grouping や schedule tuning を行わず、最小構成とした。

### open pull request 上限

各 ecosystem で、

```yaml
open-pull-requests-limit: 5
```

を設定した。

大量の更新 PR が同時に滞留することを避けつつ、通常の依存更新を止めない値としている。

### commit message prefix

repository の `.githooks/commit-msg` が Conventional Commits を前提としているため、Dependabot PR の commit prefix もそれに合わせた。

```text
npm             → chore
cargo           → chore
github-actions  → ci
```

GitHub Actions の更新は application dependency ではなく CI infrastructure の変更なので `ci` とした。

### 今回入れていないもの

以下は意図的に設定していない。

* dependency grouping
* `schedule.day`
* ecosystem ごとの細かな ignore rule
* major / minor / patch ごとの update policy
* assignee
* reviewer
* label の追加設定

初回は Dependabot が正常に repository を認識し、PR を生成できることを優先する。

実運用で PR 数や更新頻度に問題が出た場合に追加調整する。

---

## 5. 検証

### 5.1 YAML syntax

`.github/dependabot.yml` が YAML として parse 可能であることを確認した。

当初は以下で確認しようとした。

```bash
python3 -c "import yaml; yaml.safe_load(open('.github/dependabot.yml'))"
```

しかし使用中の Homebrew 管理 Python 3.14 には PyYAML が入っておらず、

```text
ModuleNotFoundError: No module named 'yaml'
```

となった。

さらに、

```bash
pip3 install pyyaml
```

は PEP 668 による externally managed environment の保護により拒否された。

```text
error: externally-managed-environment
```

system Python に対して、

```text
--break-system-packages
```

を使用する必要はないため、環境を壊す方向の回避策は採用しなかった。

最終的に YAML parse が成功することを確認した。

```bash
python3 -c "import yaml; yaml.safe_load(open('.github/dependabot.yml'))"
```

結果:

```text
YAML OK
```

### 5.2 directory の確認

repository layout と Dependabot の directory 指定が対応していることを確認した。

```text
package.json
└── /

Cargo.toml
└── /back_cargo

.github/workflows/
└── /
```

したがって、

```text
npm             /
cargo           /back_cargo
github-actions  /
```

の指定で整合している。

### 5.3 GitHub 上での実動作

Dependabot が実際に schedule に従って version check を行い、更新 PR を生成することについては repository への merge 後に GitHub 側で動作するため、ローカルだけでは完全には検証できない。

したがって以下は merge 後の実地確認項目として残す。

* Dependabot が `.github/dependabot.yml` を正常に受理する
* npm dependency update PR が生成される
* Cargo dependency update PR が生成される
* GitHub Actions update PR が生成される
* commit message prefix が期待どおりになる
* SHA pin された Actions が正常に更新される

これは Task 21 の設定実装とは分離し、GitHub 上で結果を確認する。

---

## 6. 設計上の判断

### Cargo directory は `/back_cargo`

この repository は Rust crate が repository root にない。

```text
ops-hub/
├── package.json
├── .github/
└── back_cargo/
    ├── Cargo.toml
    └── Cargo.lock
```

Dependabot の directory は package manifest が存在する directory を指定するため、Cargo は `/back_cargo` とする。

### GitHub Actions も Dependabot 対象とする

CI workflow は reusable action を full SHA pin している。

これは supply-chain 上望ましい一方、version update の追従を手作業だけにすると更新漏れが発生しやすい。

そのため GitHub Actions ecosystem を Dependabot の管理対象に含める。

### 初回から grouping しない

更新をまとめると PR 数は減るが、複数依存の変更が1 PRに混在し、失敗原因の切り分けが難しくなる。

現段階では repository 規模も限定的なため、まず Dependabot 標準の dependency 単位更新で運用を開始する。

必要性が確認されてから grouping を導入する。

---

## 7. 完了条件

以下を確認した。

* [x] `.github/dependabot.yml` を追加した
* [x] npm を `/` で監視する
* [x] Cargo を `/back_cargo` で監視する
* [x] GitHub Actions を `/` で監視する
* [x] `open-pull-requests-limit: 5` を設定した
* [x] npm の commit prefix を `chore` とした
* [x] Cargo の commit prefix を `chore` とした
* [x] GitHub Actions の commit prefix を `ci` とした
* [x] repository layout と directory 指定が一致している
* [x] YAML syntax を確認した
* [x] system Python を `--break-system-packages` で変更していない
* [x] grouping / day 指定等を意図的に初回導入範囲外とした

Task 21 完了。

GitHub 上で実際の Dependabot PR が生成されることについては merge 後の運用確認事項とする。

---

## 8. つまずいた点と教訓

### 8.1 Homebrew Python へ直接 PyYAML を install しない

Homebrew 管理の Python 3.14 では PEP 668 により system environment への直接 `pip install` が防止されている。

```text
externally-managed-environment
```

これは Python installation の破損を防ぐための正常な挙動。

単発の YAML validation のために、

```text
pip --break-system-packages
```

を使用する必要はない。

必要であれば temporary venv や既存の YAML parser を利用する。

### 8.2 Dependabot の Cargo directory は repository root とは限らない

Cargo manifest が subdirectory にある repository では、

```yaml
directory: /
```

を機械的に設定しない。

実際の `Cargo.toml` の位置に合わせて指定する。

今回の場合は、

```text
/back_cargo
```

が正しい。

### 8.3 SHA pin していても Dependabot の対象にできる

GitHub Actions の `uses:` を full commit SHA に固定していても、GitHub Actions ecosystem の Dependabot 更新対象にできる。

そのため SHA pin と自動更新は排他的ではない。

---

## 9. 次タスクへの引き継ぎ

Task 21 により Dependabot の repository 設定は導入済み。

残課題は以下。

### main-protection Ruleset の live 設定

GitHub 上の `main-protection` Ruleset と、

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

### 使用されていない scripts の整理

現在の簡略化された `Makefile` では以下が呼ばれていない。

```text
scripts/assert_local_database_url.py
scripts/check_ruleset_contract.py
scripts/run_quiet.py
```

今後、

* 品質ゲートへ戻す
* 別の明示的な verification command から使う
* 不要なら削除する

のどれを正とするか整理が必要。

### Dependabot の運用確認

merge 後に GitHub 上で以下を確認する。

```text
npm update PR
Cargo update PR
GitHub Actions update PR
```

必要に応じて、その実績を見て grouping / schedule / PR limit を再調整する。

---

