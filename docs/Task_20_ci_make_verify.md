# Task 20: ci.yml から make verify を実行

| 項目 | 内容 |
|---|---|
| 目的 | CI の `verify` job が setup だけで終了せず、repository の正規品質ゲート `make verify` を実際に実行する |
| 完了条件 | GitHub Actions の `verify` job で `make verify` が PASS する |
| 採番 | 旧採番 #9 から繰り上げ |
| 前提 | Task 17（`vite.config.ts` 復元）/ Task 18（`back_cargo/` 復元）/ Task 19（v3 残骸削除）適用済み |
| ステータス | 完了 |

## 1. 実施内容

### 背景

Task 20 着手前の `verify` job は checkout / Rust cache / Node.js setup / `npm ci` / `sqlx-cli` install までしか行っていなかった。直近の CI run が green でも「依存関係とツールのセットアップに成功した」ことしか示しておらず、lint / compile / SQLx metadata / test は実行されていなかった。

### 変更内容

1. **CI 用 `DATABASE_URL` を追加** — `postgres://ops_hub:ops_hub@localhost:5432/ops_hub`。ローカルは port 競合回避のため `5433`、GitHub Actions runner は job 専有環境で PostgreSQL service が `5432:5432` なので `5432` に固定
2. **`verify` job の最後で `make verify` を実行**
3. **`Makefile` で Rust crate の位置を明示**（`CARGO_DIR := back_cargo`）
4. **CI 用 PostgreSQL に migration を適用する step を追加**（`cargo sqlx migrate run`、`make verify` の前）
5. **Ruleset / CI job 名の契約チェックを強化** — `scripts/check_ruleset_contract.py` に `job_name()` を追加し、`jobs.verify.name == "verify"` を直接検証。`on:` に `paths` / `paths-ignore` が無いことも検証

Makefile（Task 20 時点）：

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

> Task 22 で `assert_local_database_url.py` / `check_ruleset_contract.py` の呼び出しが `verify` の先頭に再導入されている。

最終的な `verify` job：

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

> action の SHA は Task 20 時点のもの。以降 Dependabot により更新されている。

### 検証結果

- [x] CI 用 `DATABASE_URL` が PostgreSQL service の `5432` を使用する
- [x] `verify` job の最後で `make verify` を実行する
- [x] CI 側へ品質検証コマンドを重複定義していない
- [x] repository root から `make verify` を実行できる
- [x] Rust commands は `back_cargo/` で実行される
- [x] CI ephemeral PostgreSQL に migration を適用してから品質検証する
- [x] SQLx compile-time query validation が実 schema に対して成功する
- [x] `jobs.verify.name` が `verify` のまま維持され、Ruleset required status check `verify` と一致
- [x] `paths` / `paths-ignore` による required check skip がない
- [x] Ruleset / CI contract check PASS
- [x] Python tooling tests PASS
- [x] ローカル `make verify` PASS
- [x] GitHub Actions の `verify` job PASS

## 2. 設計判断

### CI とローカルで `make verify` を共通化する

品質ゲートの正本は `Makefile` とする。CI workflow に `cargo fmt` / `clippy` / `test` 等を個別コピーすると、ローカルと CI の品質ゲートが将来的に乖離する。

```text
Makefile
  └── verify
        ↑
        ├── developer
        └── GitHub Actions
```

### Rust crate の位置は Makefile が知る

`back_cargo/` は repository 内の Rust crate location であり、CI 固有の情報ではない。各 step に `working-directory: back_cargo` を付けるのではなく、Makefile が `CARGO_DIR := back_cargo` を保持する。これにより `make verify` はどの環境でも repository root から実行できる。

### CI database migration は CI setup の責務

migration は `make verify` に組み込まず CI setup として実行する。

- GitHub Actions の PostgreSQL は毎 run 空の database
- SQLx compile-time query validation 前に schema が必要
- `make verify` が database schema を暗黙に変更する設計にはしない
- ローカルで誤った `DATABASE_URL` に対して `make verify` を実行した際の schema mutation を避ける

## 3. つまずいた点と教訓

### #1 CI で `make verify` を呼ぶだけでは crate directory は解決されない

初回実行で失敗：

```text
cargo metadata exited with an error:
could not find `Cargo.toml` in
`/home/runner/work/ops-hub/ops-hub`
or any parent directory
```

`Makefile` は repository root、`Cargo.toml` は `back_cargo/` 配下にある。**品質ゲートを repository root から実行するなら、Makefile 側で crate location を明示する。**

### #2 SQLx macro は test 前でも database schema を必要とする

crate path 修正後の実行で失敗：

```text
error returned from database:
relation "runs" does not exist

src/repository/run_repo.rs
```

`sqlx::query!` / `query_scalar!` は compile-time validation を行うため、`cargo clippy` の段階で schema が必要。`#[sqlx::test]` が test database に migration を適用することとは別問題。**CI の ephemeral PostgreSQL は `make verify` より前に migration 済みである必要がある。**

### #3 required status check は job ID ではなく表示 context も契約になる

Ruleset が要求する `verify` と CI の `jobs.verify.name` は独立して変更可能。従来のチェックは `jobs:` 配下のどこかに `name: verify` があれば PASS していたため、以下の誤構成を検知できなかった。

```yaml
jobs:
  verify:
    name: something-else

  dummy:
    name: verify
```

**`jobs.verify.name == verify` まで固定することで、意図しない job rename / replacement を検知できる。**

## 4. 再現コマンド

```bash
# Ruleset / CI 契約チェック
python3 scripts/check_ruleset_contract.py --ci .github/workflows/ci.yml
# ruleset contract check passed:
# contexts=['verify'], jobs.verify.name='verify', no paths/paths-ignore triggers

# Python tooling tests
python3 -m unittest discover -s scripts/tests -p 'test_*.py'

# ローカル品質ゲート（repository root から）
make verify
```

## 5. 次タスクへの引き継ぎ

- Dependabot（`.github/dependabot.yml`）の導入
- `main-protection` Ruleset の live 設定と `scripts/github/main-ruleset.json` の差分解消
  - `strict_required_status_checks_policy`
  - `required_review_thread_resolution`
  - `require_extra_approval_for_unattributed_changes`
- 次タスクでは CI の内部処理ではなく、GitHub Ruleset の live 設定を正本 JSON と一致させる
