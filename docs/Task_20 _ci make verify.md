# Task 20: ci.yml から make verify を実行
（旧採番 #9 から繰り上げ）

## 1. 背景

`.github/workflows/ci.yml` の `verify` job は、これまで以下のセットアップまでしか行っていなかった。

- checkout
- Rust cache 初期化
- Node.js setup
- `npm ci`
- `sqlx-cli` install

そのため、直近の CI run が green であっても、実質的には「依存関係とツールのセットアップに成功した」ことしか保証しておらず、lint / compile / build / SQLx metadata / test の検証は行われていなかった。

Task 19 で AI 協働フレームワーク v3 の残骸を削除し、`Makefile` の品質ゲートも整理済みである。

現在の `make verify` は、Rust / SQLx / frontend / repository tooling に必要な検証を一括実行する正本となっている。

CI 側に個別コマンドを重複定義すると、ローカルと CI の検証内容が将来的に乖離するため、`.github/workflows/ci.yml` では `make verify` のみを最終品質ゲートとして呼び出す方針とした。

---

## 2. 実施したこと

### 2.1 CI用 DATABASE_URL を明示

`.github/workflows/ci.yml` の `jobs.verify.env` に以下を追加した。

```yaml
env:
  DATABASE_URL: postgres://ops_hub:ops_hub@localhost:5432/ops_hub
````

ローカル開発環境では複数プロジェクトとの PostgreSQL port 競合を避けるため `5433` を使用するが、GitHub Actions runner は job ごとの専有環境なので、既存の PostgreSQL service、

```yaml
ports:
  - 5432:5432
```

に合わせて CI では `5432` に固定した。

これにより、

```text
local development
  localhost:5433

GitHub Actions
  localhost:5432
```

という役割分担を明確化した。

### 2.2 CIの最終stepで `make verify` を実行

`verify` job の最後に以下を追加した。

```yaml
- name: Verify
  run: make verify
```

CI workflow 側に `cargo fmt` / `cargo clippy` / `cargo test` / `npm run lint` 等を個別列挙せず、`Makefile` を品質ゲートの正本として使用する。

これにより、

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

という構造になった。

### 2.3 Ruleset / CI job名の契約チェックを強化

`scripts/check_ruleset_contract.py` について、Ruleset の required status check と CI workflow の対応をより厳密に検証するよう更新した。

従来は `jobs:` 配下のどこかに、

```yaml
name: verify
```

が存在すれば PASS していたため、例えば以下のような誤った構成でも契約チェックを通過できた。

```yaml
jobs:
  verify:
    name: something-else

  dummy:
    name: verify
```

この問題を防ぐため、`job_name()` を追加し、

```yaml
jobs:
  verify:
    name: verify
```

を直接検証するようにした。

現在の静的契約は以下。

```text
scripts/github/main-ruleset.json
required_status_checks[].context
             │
             └── "verify"
                    │
                    ▼
.github/workflows/ci.yml
jobs.verify.name
             │
             └── "verify"
```

さらに `on:` ブロックに、

```yaml
paths:
paths-ignore:
```

が存在しないことも継続して検証する。

これは path filter により `verify` job 自体が起動せず、Ruleset の required check が永久に pending になる事故を防ぐための契約である。

---

## 3. 最終状態

`.github/workflows/ci.yml` の `verify` job は以下の構造になった。

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

      - name: Verify
        run: make verify
```

---

## 4. 検証

### 4.1 Ruleset / CI契約

以下を実行。

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

これにより以下を確認した。

* `main-ruleset.json` の required status check context が `verify`
* `.github/workflows/ci.yml` に `jobs.verify` が存在する
* `jobs.verify.name` が厳密に `verify`
* `on:` に `paths` が存在しない
* `on:` に `paths-ignore` が存在しない

### 4.2 Python tooling tests

`scripts/run_quiet.py` 等を含む repository tooling の unit test を実行。

```bash
python3 -m unittest discover \
  -s scripts/tests \
  -p 'test_*.py'
```

PASS。

### 4.3 `make verify`

最終品質ゲートを実行。

```bash
make verify
```

PASS。

Rust / SQLx / frontend / repository tooling を含む現行 `Makefile` の品質ゲートがすべて成功することを確認した。

したがって、以前の

> cargo / rustc が無いサンドボックス環境のため Rust 関連ステップは未確認

という制約は解消済みであり、本タスクのローカル検証は完了した。

---

## 5. Rulesetとの整合性

`scripts/github/main-ruleset.json` では `main` に対して required status check として以下を要求する。

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

CI側も、

```yaml
jobs:
  verify:
    name: verify
```

となっているため、Ruleset が要求する status check context と実際の GitHub Actions job 名が一致している。

この関係は `scripts/check_ruleset_contract.py` によって静的に検証される。

---

## 6. 前提条件

本タスクは以下の変更が適用済みであることを前提とする。

1. Task 17 — `vite.config.ts` 復元
2. Task 18 — `back_cargo/` 復元
3. Task 19 — AI協働フレームワーク v3 残骸の削除と `Makefile` 整理

特に `back_cargo/` が存在しない状態では、`make verify` 内の Rust 系検証が成立しない。

また Task 19 により、旧AI scaffold由来の、

```text
scripts/check_task_docs.py
task-index
```

を品質ゲートへ組み込む構造は廃止済みである。

`Makefile` の `verify` は現在のリポジトリに必要な検証のみを実行する。

---

## 7. 完了判定

Task 20 の目的である、

> CI の `verify` job がセットアップだけで終了せず、リポジトリの正規品質ゲート `make verify` を実際に実行する

ための変更は完了した。

確認済み:

* [x] CI用 `DATABASE_URL` が PostgreSQL service の `5432` を使用する
* [x] `verify` job の最後で `make verify` を実行する
* [x] CIに個別品質コマンドを重複定義していない
* [x] `jobs.verify.name` は `verify` のまま維持されている
* [x] Ruleset required status check `verify` と一致している
* [x] `paths` / `paths-ignore` により required check がskipされる構成になっていない
* [x] Ruleset / CI静的契約チェックがPASS
* [x] Python tooling testsがPASS
* [x] `make verify` がPASS

Task 20 完了。

---

## 8. 次タスクへの引き継ぎ

Task 20 により CI の `verify` job は実質的な品質ゲートになった。

残作業は本タスク外として以下。

* Dependabot (`.github/dependabot.yml`) の導入
* `main-protection` Ruleset の live 設定と `scripts/github/main-ruleset.json` の差分解消

  * `strict_required_status_checks_policy`
  * `required_review_thread_resolution`
  * `require_extra_approval_for_unattributed_changes`
* GitHub上の branch protection / Ruleset を期待状態へ収束させる作業

次タスクでは、CIそのものではなく GitHub Ruleset の live 設定を正本JSONへ一致させる。

```

今回の更新では、旧版の「Rust関連検証は未実施」という記述を削除し、`make verify` を含めて全検証成功へ変更しています。また、先ほど追加した `jobs.verify.name == "verify"` の厳密な契約チェックも Task 20 の実施・検証内容へ統合しています。
```
