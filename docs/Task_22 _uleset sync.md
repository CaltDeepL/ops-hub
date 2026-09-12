# Task 22: main-protection Ruleset の正本と live 設定を一致させる

## 1. 背景

Task 21 完了時点で、GitHub の `main-protection` Ruleset と、

```text
scripts/github/main-ruleset.json
````

に以下の差分が残っていた。

| 設定                                                | GitHub live |      正本 |
| ------------------------------------------------- | ----------: | ------: |
| `strict_required_status_checks_policy`            |      `true` | `false` |
| `required_review_thread_resolution`               |      `true` | `false` |
| `require_extra_approval_for_unattributed_changes` |      `true` | `false` |

当初は `main-ruleset.json` を正本として live を `false` に合わせる予定だった。

しかし3項目とも live 側の方が厳しく、特に `strict_required_status_checks_policy=true` は、PR を最新 `main` に追従させてから merge するための保護になる。

Task 18 で古い branch との merge 解決ミスにより `back_cargo/` が消失した事故も踏まえ、live の保護を弱めず、repository 側の正本を live に合わせる方針へ変更した。

---

## 2. 実施したこと

### 2.1 `main-ruleset.json` を live に合わせた

以下をすべて `true` に変更した。

```json
"strict_required_status_checks_policy": true
"required_review_thread_resolution": true
"require_extra_approval_for_unattributed_changes": true
```

これにより、

```text
scripts/github/main-ruleset.json
        =
GitHub live Ruleset
```

となった。

---

### 2.2 `sync_main_ruleset.py` を追加

将来 repository 内の正本から live Ruleset を同期できるよう、

```text
scripts/github/sync_main_ruleset.py
```

を追加した。

主な機能:

* desired / live の差分表示
* `rules` を `type` 単位で比較
* `--dry-run`
* y/N 確認付き PUT
* `--yes`
* 403 / 404 の補助表示
* `verify_main_ruleset.py` の比較ロジックを再利用

Task 22 では live 側を変更しない方針にしたため、実際の PUT は行っていない。

---

### 2.3 GitHub CLI を認証

最初は、

```bash
gh auth token
```

が、

```text
no oauth token found for github.com
```

となった。

`gh auth login` で GitHub CLI を認証し、

```bash
export GITHUB_TOKEN="$(gh auth token)"
```

で認証済み API を利用できる状態にした。

`git` の認証と `gh` の認証は別である点を確認した。

---

### 2.4 live Ruleset を確認

認証済み API で以下を確認した。

```text
main-protection: active

strict_required_status_checks_policy = true
required_review_thread_resolution = true
require_extra_approval_for_unattributed_changes = true

bypass_actors = []
```

管理者を含む bypass actor は設定されていない。

---

## 3. 未使用 script の整理

Task 22 着手時点では以下が `Makefile` から外れていた。

```text
scripts/assert_local_database_url.py
scripts/check_ruleset_contract.py
scripts/run_quiet.py
```

判断は以下。

```text
assert_local_database_url.py
    → make verify に再導入

check_ruleset_contract.py
    → make verify に再導入

run_quiet.py
    → 再導入しない
```

### `assert_local_database_url.py`

`DATABASE_URL` が以下の local / CI host だけを指していることを検証する。

```text
localhost
127.0.0.1
db
postgres
```

Neon 等の remote DB へ誤って `make verify` を実行することを防ぐ。

### `check_ruleset_contract.py`

Ruleset の required check と CI job 名を静的に検証する。

```text
main-ruleset.json
required check = verify
        ↓
ci.yml
jobs.verify.name = verify
```

また `paths` / `paths-ignore` により required check が生成されなくなる構成も拒否する。

### `run_quiet.py`

ログ短縮用 wrapper であり、検証内容自体は増えないため再導入しない。

---

## 4. 最終的な `make verify`

```makefile
.PHONY: verify sqlx-prepare

CARGO_DIR := back_cargo

verify:
	python3 scripts/assert_local_database_url.py
	python3 scripts/check_ruleset_contract.py
	cd $(CARGO_DIR) && \
		cargo fmt --all -- --check && \
		cargo clippy --all-targets --all-features -- -D warnings && \
		cargo sqlx prepare --check -- --all-targets --all-features && \
		cargo test --all-targets --all-features

sqlx-prepare:
	cd $(CARGO_DIR) && \
		cargo sqlx prepare -- --all-targets --all-features
```

Task 20 で確立したとおり、CI も引き続き、

```yaml
- name: Verify
  run: make verify
```

を品質ゲートの単一入口とする。

---

## 5. merge gate の実証

設定確認だけでなく、実際の PR で Ruleset が機能することを確認した。

### failure path

意図的に `verify` を失敗させた。

```text
verify FAILED
      ↓
required status check 未達
      ↓
merge BLOCKED
```

`verify` が失敗している状態では main へ merge できないことを確認した。

### success path

failure を除去し、再度 CI を実行した。

```text
latest main
      +
verify PASS
      +
Ruleset 条件成立
      ↓
merge ALLOWED
```

`verify` が成功し、必要な Ruleset 条件を満たすと merge 可能になることを確認した。

---

## 6. 検証結果

以下をすべて確認した。

* [x] GitHub live Ruleset を認証済み API で確認
* [x] `main-protection` が active
* [x] `bypass_actors = []`
* [x] 3つの保護設定を `true` で統一
* [x] `main-ruleset.json` と live が一致
* [x] `sync_main_ruleset.py` を追加
* [x] `check_ruleset_contract.py` を `make verify` に再導入
* [x] `assert_local_database_url.py` を `make verify` に再導入
* [x] `run_quiet.py` は再導入しない方針を確定
* [x] Ruleset required check と `jobs.verify.name` が一致
* [x] ローカル `make verify` PASS
* [x] GitHub Actions `verify` PASS
* [x] failure path で merge block を確認
* [x] success path で merge 可能を確認

Task 22 完了。

---

## 7. Task 22 完了時点の main protection

```text
main
├── deletion prohibited
├── non-fast-forward prohibited
├── pull request required
├── required status check: verify
├── strict status checks: true
├── review thread resolution required
├── extra approval for unattributed changes required
└── bypass actors: none
```

基本的な merge 経路は、

```text
feature branch
      ↓
Pull Request
      ↓
latest main に追従
      ↓
verify PASS
      ↓
merge
```

となる。

Task 18 の古い branch 起因の事故に対しても、最新 `main` 上で CI を通してから merge する方向へ保護を強化した。

---

## 8. つまずいた点と教訓

### live と正本に差分があっても、機械的に live を変更しない

今回は live 側の方が安全だったため、

```text
live → repository 正本
```

へ収束させた。

drift の解消より先に、どちらの設定が望ましいか判断する必要がある。

### `git` と `gh` の認証は別

Git操作ができても `gh auth token` が使えるとは限らない。

GitHub API を利用する場合は、

```bash
gh auth status
```

を確認する。

### Ruleset は failure / success path まで確認する

静的設定だけでなく、

```text
verify FAIL → merge BLOCKED
verify PASS → merge ALLOWED
```

まで実証して初めて、required check が実際の merge gate として機能していることを確認できる。

---

## 9. 次タスクへの引き継ぎ

Task 22 固有の Ruleset 調整は完了。

今後 Ruleset を変更する場合は、

```text
1. main-ruleset.json を変更
2. check_ruleset_contract.py
3. sync_main_ruleset.py --dry-run
4. live / bypass_actors を確認
5. 必要なら適用
6. verify_main_ruleset.py で再確認
```

の順で扱う。

Task 22 完了。
