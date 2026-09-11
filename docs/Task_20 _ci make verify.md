# Task 20: ci.yml から make verify を実行
（旧採番 #9 から繰り上げ）

## 1. 背景

`.github/workflows/ci.yml` の `verify` job は、Task 20 着手前は以下のセットアップまでしか行っていなかった。

- checkout
- Rust cache 初期化
- Node.js setup
- `npm ci`
- `sqlx-cli` install

このため、直近の CI run が green であっても、それは「依存関係とツールのセットアップに成功した」ことしか示しておらず、実際の lint / compile / build / SQLx metadata / test は実行されていなかった。

Task 19 で AI 協働フレームワーク v3 の残骸を削除した後、`Makefile` には現行リポジトリで必要な品質検証をまとめた `verify` target が残っている。

CI 側に `cargo fmt` / `cargo clippy` / `cargo test` / frontend lint/build 等を個別列挙すると、ローカルと CI で品質ゲートが将来的に乖離するため、CI では `make verify` を正本としてそのまま実行する設計とした。

---

## 2. 実施したこと

### 2.1 CI用 `DATABASE_URL` を追加

`.github/workflows/ci.yml` の `verify` job に以下を追加した。

```yaml
env:
  DATABASE_URL: postgres://ops_hub:ops_hub@localhost:5432/ops_hub
````

ローカル開発環境では複数プロジェクトとの PostgreSQL port 競合を避けるため `5433` を使用している。

一方 GitHub Actions runner は job ごとの専有環境であり、既存の PostgreSQL service も、

```yaml
ports:
  - 5432:5432
```

としているため、CI では `localhost:5432` に固定した。

役割分担は以下。

```text
local development
  PostgreSQL: localhost:5433

GitHub Actions
  PostgreSQL: localhost:5432
```

---

### 2.2 `verify` job の最後で `make verify` を実行

CI の最後の step として以下を追加した。

```yaml
- name: Verify
  run: make verify
```

これにより、CI workflow 側へ品質検証コマンドを重複記述せず、ローカルと CI が同じ品質ゲートを使用する構成にした。

```text
local
  make verify
      │
      └── 品質ゲート

CI
  make verify
      │
      └── 同じ品質ゲート
```

---

### 2.3 `Makefile` の Rust crate 実行位置を修正

最初の CI 実行では、`make verify` の最初の Rust command で失敗した。

```text
cargo metadata exited with an error:
could not find `Cargo.toml` in
`/home/runner/work/ops-hub/ops-hub`
or any parent directory
```

原因は、`Makefile` が repository root に存在する一方、Rust crate の `Cargo.toml` は `back_cargo/` 配下に存在するためだった。

構成は以下。

```text
ops-hub/
├── Makefile
├── package.json
├── scripts/
├── .github/
└── back_cargo/
    ├── Cargo.toml
    ├── Cargo.lock
    └── src/
```

Task 20 着手前の `Makefile` は repository root から直接、

```makefile
cargo fmt --all -- --check
```

等を実行していたため、CI では `Cargo.toml` を発見できなかった。

CI 側に `working-directory: back_cargo` を付けるのではなく、`make verify` 自体を repository root から実行可能な品質ゲートにするため、`Makefile` 側で crate directory を明示した。

```makefile
.PHONY: verify sqlx-prepare

CARGO_DIR := back_cargo

verify:
	cd $(CARGO_DIR) && \
		cargo fmt --all -- --check && \
		cargo clippy --all-targets --all-features -- -D warnings && \
		cargo sqlx prepare --check -- --all-targets --all-features && \
		cargo test --all-targets --all-features

sqlx-prepare:
	cd $(CARGO_DIR) && \
		cargo sqlx prepare -- --all-targets --all-features
```

これにより、

```bash
make verify
```

は常に repository root から実行するという契約に統一された。

---

### 2.4 CI用 PostgreSQL に migration を適用

`Makefile` の crate path 修正後、次の CI 実行では SQLx compile-time query validation で失敗した。

代表例:

```text
error returned from database:
relation "runs" does not exist

src/repository/run_repo.rs
```

失敗した query は以下のように `runs` table を参照している。

```sql
INSERT INTO runs DEFAULT VALUES
RETURNING id
```

同様に、

```sql
SELECT ... FROM runs
UPDATE runs ...
```

も compile-time validation で失敗した。

原因は、GitHub Actions の PostgreSQL service が毎 run 空の database として起動する一方、`make verify` 実行前に migration を適用していなかったため。

`sqlx::query!` / `sqlx::query_scalar!` は compile-time に `DATABASE_URL` の database schema を参照するため、`cargo clippy` の時点ですでに必要な table が存在している必要がある。

そのため、`sqlx-cli` install 後、`make verify` より前に CI database 初期化 step を追加した。

```yaml
- name: Run database migrations
  working-directory: back_cargo
  run: cargo sqlx migrate run
```

最終的な順序は以下。

```text
PostgreSQL service 起動
        ↓
Checkout
        ↓
Rust cache
        ↓
Node setup
        ↓
npm ci
        ↓
sqlx-cli install
        ↓
cargo sqlx migrate run
        ↓
make verify
```

migration 実行は品質ゲートそのものではなく、CI の ephemeral PostgreSQL を検証可能な schema 状態へ初期化する setup として CI workflow 側に置いた。

`make verify` は引き続き最終品質ゲートの正本とする。

---

### 2.5 Ruleset / CI job名の契約チェックを強化

`scripts/check_ruleset_contract.py` では、`scripts/github/main-ruleset.json` の required status check と `.github/workflows/ci.yml` の対応を静的に検証している。

Ruleset 側は、

```json
{
  "type": "required_status_checks",
  "parameters": {
    "required_status_checks": [
      {
        "context": "verify",
        "integration_id": 15368
      }
    ]
  }
}
```

となっている。

CI 側は、

```yaml
jobs:
  verify:
    name: verify
```

である。

従来の契約チェックでは `jobs:` 配下のどこかに `name: verify` が存在すれば PASS していたため、例えば以下の誤構成も検知できなかった。

```yaml
jobs:
  verify:
    name: something-else

  dummy:
    name: verify
```

そのため `job_name()` を追加し、以下を直接検証するよう強化した。

```text
jobs.verify.name == "verify"
```

現在の契約は以下。

```text
scripts/github/main-ruleset.json
required_status_checks[].context
             │
             └── verify
                    │
                    ▼
.github/workflows/ci.yml
jobs.verify.name
             │
             └── verify
```

加えて、`on:` block に以下が存在しないことも検証する。

```yaml
paths:
paths-ignore:
```

required status check 対象 workflow が path filter により起動せず、Ruleset 上で required check が pending のままになる事故を防ぐため。

---

## 3. 最終的な CI 構成

`verify` job は最終的に以下の構成となった。

```yaml
jobs:
  verify:
    name: verify
    runs-on: ubuntu-latest
    timeout-minutes: 20

    env:
      DATABASE_URL: postgres://ops_hub:ops_hub@localhost:5432/ops_hub

    services:
      postgres:
        image: postgres:17-bookworm
        env:
          POSTGRES_USER: ops_hub
          POSTGRES_PASSWORD: ops_hub
          POSTGRES_DB: ops_hub
        ports:
          - 5432:5432
        options: >-
          --health-cmd "pg_isready -U ops_hub -d ops_hub"
          --health-interval 5s
          --health-timeout 5s
          --health-retries 10

    steps:
      - name: Checkout
        uses: actions/checkout@d23441a48e516b6c34aea4fa41551a30e30af803 # v6

      - name: Rust cache
        uses: Swatinem/rust-cache@6323deb102c322ba6fcbdcafc7e3dddab59af2b6 # v2.9.2
        with:
          workspaces: back_cargo -> target

      - name: Setup Node
        uses: actions/setup-node@249970729cb0ef3589644e2896645e5dc5ba9c38 # v6
        with:
          node-version: "24"
          cache: npm
          cache-dependency-path: package-lock.json

      - name: Install frontend dependencies
        run: npm ci

      - name: Install sqlx-cli
        uses: taiki-e/install-action@5bf6ce016fd2e72eefc647cbca1e4213f65955b8 # v2.87.5
        with:
          tool: sqlx-cli@0.9

      - name: Run database migrations
        working-directory: back_cargo
        run: cargo sqlx migrate run

      - name: Verify
        run: make verify
```

---

## 4. 検証

### 4.1 Ruleset / CI 契約チェック

実行:

```bash
python3 scripts/check_ruleset_contract.py \
  --ci .github/workflows/ci.yml
```

結果:

```text
ruleset contract check passed:
contexts=['verify'],
jobs.verify.name='verify',
no paths/paths-ignore triggers
```

PASS。

以下を確認した。

* `main-ruleset.json` の required status check context が `verify`
* `jobs.verify` が存在する
* `jobs.verify.name` が厳密に `verify`
* `on:` に `paths` が存在しない
* `on:` に `paths-ignore` が存在しない

---

### 4.2 Python tooling tests

実行:

```bash
python3 -m unittest discover \
  -s scripts/tests \
  -p 'test_*.py'
```

PASS。

`run_quiet.py` を含む repository tooling の unit tests が成功することを確認した。

---

### 4.3 ローカル `make verify`

repository root から実行。

```bash
make verify
```

PASS。

`back_cargo/` を明示する Makefile 構成で、repository root から品質ゲート全体を実行できることを確認した。

---

### 4.4 GitHub Actions

初回実行では repository root から `cargo` を実行したため `Cargo.toml` を発見できず FAIL。

```text
could not find `Cargo.toml`
```

Makefile の `CARGO_DIR := back_cargo` 対応後、次の実行では database migration 未適用により FAIL。

```text
relation "runs" does not exist
```

CI に、

```yaml
- name: Run database migrations
  working-directory: back_cargo
  run: cargo sqlx migrate run
```

を追加後、`verify` job が正常完走することを確認した。

最終結果:

```text
GitHub Actions verify: PASS
```

これにより、「CI が setup だけで green になる」という Task 20 着手前の問題は解消された。

---

## 5. 設計上の判断

### CI とローカルで `make verify` を共通化する

品質ゲートの正本は `Makefile` とする。

CI workflow に品質検証コマンドを個別コピーしない。

```text
Makefile
  └── verify
        ↑
        ├── developer
        └── GitHub Actions
```

---

### Rust crate の位置は Makefile が知る

`back_cargo/` は repository 内の Rust crate location であり、CI 固有の情報ではない。

そのため、

```yaml
working-directory: back_cargo
```

を各品質検証 step に付与するのではなく、Makefile が、

```makefile
CARGO_DIR := back_cargo
```

を保持する。

これにより `make verify` をどの環境でも repository root から実行できる。

---

### CI database migration は CI setup の責務

migration は `make verify` に組み込まず CI setup として実行する。

理由:

* GitHub Actions の PostgreSQL は毎 run 空の database
* SQLx compile-time query validation 前に schema が必要
* `make verify` が database schema を暗黙に変更する設計にはしない
* ローカルで誤った `DATABASE_URL` に対して `make verify` を実行した際の schema mutation を避ける

したがって、

```text
CI setup
  └── cargo sqlx migrate run

quality gate
  └── make verify
```

と責務を分離した。

---

## 6. 前提条件

Task 20 は以下の変更が適用済みであることを前提とする。

1. Task 17 — `vite.config.ts` 復元
2. Task 18 — `back_cargo/` 復元
3. Task 19 — AI協働フレームワーク v3 残骸の削除

特に `back_cargo/` が存在しない状態では Rust 系品質検証は成立しない。

Task 19 により、旧 AI scaffold 由来の以下は品質ゲートから削除済み。

```text
scripts/check_task_docs.py
task-index
```

現在の `make verify` は現行 repository に必要な検証のみを実行する。

---

## 7. 完了条件

Task 20 の目的:

> CI の `verify` job が setup だけで終了せず、repository の正規品質ゲート `make verify` を実際に実行する

について、以下をすべて確認した。

* [x] CI用 `DATABASE_URL` が PostgreSQL service の `5432` を使用する
* [x] `verify` job の最後で `make verify` を実行する
* [x] CI側へ品質検証コマンドを重複定義していない
* [x] repository root から `make verify` を実行できる
* [x] Rust commands は `back_cargo/` で実行される
* [x] CI ephemeral PostgreSQL に migration を適用してから品質検証する
* [x] SQLx compile-time query validation が実 schema に対して成功する
* [x] `jobs.verify.name` が `verify` のまま維持されている
* [x] Ruleset required status check `verify` と一致している
* [x] `paths` / `paths-ignore` による required check skip がない
* [x] Ruleset / CI contract check が PASS
* [x] Python tooling tests が PASS
* [x] ローカル `make verify` が PASS
* [x] GitHub Actions の `verify` job が PASS

Task 20 完了。

---

## 8. つまずいた点と教訓

### 8.1 CIで `make verify` を呼ぶだけでは crate directory は解決されない

repository root に `Cargo.toml` がない monorepo / subdirectory crate 構成では、単純な、

```makefile
cargo fmt
```

は CI で失敗する。

品質ゲートを repository root から実行するなら、Makefile 側で crate location を明示する。

---

### 8.2 SQLx macro は test 前でも database schema を必要とする

`sqlx::query!` / `query_scalar!` は compile-time validation を行うため、

```text
cargo clippy
```

の段階でも database schema が必要になる。

`#[sqlx::test]` が test database に migration を適用することとは別問題。

CI の ephemeral PostgreSQL は `make verify` より前に migration 済みである必要がある。

---

### 8.3 required status check は job ID ではなく表示 context も契約になる

Ruleset が要求する、

```text
verify
```

と CI の、

```yaml
jobs:
  verify:
    name: verify
```

は独立して変更可能である。

そのため `scripts/check_ruleset_contract.py` で静的契約として検証する。

単に「どこかの job が `name: verify`」ではなく、

```text
jobs.verify.name == verify
```

まで固定することで意図しない job rename / replacement を検知できる。

---

## 9. 次タスクへの引き継ぎ

Task 20 により CI の `verify` job は実質的な品質ゲートになった。

今後の残作業は本タスク外。

* Dependabot (`.github/dependabot.yml`) の導入
* `main-protection` Ruleset の live 設定と `scripts/github/main-ruleset.json` の差分解消

  * `strict_required_status_checks_policy`
  * `required_review_thread_resolution`
  * `require_extra_approval_for_unattributed_changes`
* GitHub上の Ruleset を repository 内の期待値へ収束させる

次タスクでは CI の内部処理ではなく、GitHub Ruleset の live 設定を正本 JSON と一致させる。

````

今回の重要な追加点は、Task 20 を単なる「`run: make verify` 追加」で終わらせず、実際のCI実行で発覚した2つの不整合も履歴として残したことです。

```text
1回目
Cargo.toml の場所が違う
→ Makefile に CARGO_DIR := back_cargo

2回目
CI DB に migration がない
→ cargo sqlx migrate run を追加

最終
make verify / Ruleset contract / tooling tests / GitHub Actions
→ all PASS
````


