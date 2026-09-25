# Task 22: main-protection Ruleset の正本と live 設定を一致させる

| 項目 | 内容 |
|---|---|
| 目的 | GitHub の `main-protection` Ruleset と repository 内の正本 `scripts/github/main-ruleset.json` の差分を解消する |
| 完了条件 | 正本と live が一致し、実際の PR で `verify FAIL → merge BLOCKED` / `verify PASS → merge ALLOWED` を確認する |
| ステータス | 完了 |

## 1. 実施内容

### 着手時点の差分

| 設定 | GitHub live | 正本 |
|---|---:|---:|
| `strict_required_status_checks_policy` | `true` | `false` |
| `required_review_thread_resolution` | `true` | `false` |
| `require_extra_approval_for_unattributed_changes` | `true` | `false` |

### 変更内容

1. **`main-ruleset.json` を live に合わせて3項目を `true` に変更**（`main-ruleset.json = GitHub live` になった）
2. **`scripts/github/sync_main_ruleset.py` を追加** — desired / live の差分表示、`rules` を `type` 単位で比較、`--dry-run`、y/N 確認付き PUT、`--yes`、403 / 404 の補助表示。比較ロジックは `verify_main_ruleset.py` を再利用。本タスクでは live を変更しない方針のため PUT は実行していない
3. **GitHub CLI を認証し live を確認** — `main-protection: active`、3項目 `true`、`bypass_actors = []`
4. **未使用 script の整理**
   - `assert_local_database_url.py` → `make verify` に再導入（Neon 等の remote DB に対する誤実行を防ぐ。許可 host は `localhost` / `127.0.0.1` / `db` / `postgres`）
   - `check_ruleset_contract.py` → `make verify` に再導入（required check と `jobs.verify.name` の一致、`paths` / `paths-ignore` の拒否）
   - `run_quiet.py` → 再導入しない（ログ短縮用 wrapper で、検証内容は増えない）

最終的な `make verify`：

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

### merge gate の実証

```text
failure path:  verify FAILED → required status check 未達 → merge BLOCKED
success path:  latest main + verify PASS + Ruleset 条件成立 → merge ALLOWED
```

### Task 22 完了時点の main protection

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

### 検証結果

- [x] GitHub live Ruleset を認証済み API で確認
- [x] `main-protection` が active、`bypass_actors = []`
- [x] 3つの保護設定を `true` で統一し、`main-ruleset.json` と live が一致
- [x] `sync_main_ruleset.py` を追加
- [x] `check_ruleset_contract.py` / `assert_local_database_url.py` を `make verify` に再導入
- [x] `run_quiet.py` は再導入しない方針を確定
- [x] Ruleset required check と `jobs.verify.name` が一致
- [x] ローカル `make verify` PASS / GitHub Actions `verify` PASS
- [x] failure path で merge block、success path で merge 可能を確認

## 2. 設計判断

### 正本を live に合わせる（live を弱めない）

当初は `main-ruleset.json` を正本として live を `false` に合わせる予定だった。しかし3項目とも live 側の方が厳しく、特に `strict_required_status_checks_policy=true` は PR を最新 `main` に追従させてから merge するための保護になる。Task 18 で古い branch との merge 解決ミスにより `back_cargo/` が消失した事故も踏まえ、**live の保護を弱めず、repository 側の正本を live に合わせた。**

### 同期ツールは用意するが、今回は適用しない

将来 repository 内の正本から live を同期できるよう `sync_main_ruleset.py` を用意したが、今回は live を変更する必要がないため PUT は行わない。

### `make verify` を品質ゲートの単一入口に保つ

Task 20 で確立したとおり、CI も引き続き `make verify` を実行する。安全ガードと契約チェックも `make verify` 内に置くことで、ローカルと CI で同じ検証が走る。

## 3. つまずいた点と教訓

### #1 live と正本に差分があっても、機械的に live を変更しない

drift の解消より先に、どちらの設定が望ましいか判断する。今回は live 側の方が安全だったため、`live → repository 正本` へ収束させた。

### #2 `git` と `gh` の認証は別

`gh auth token` が `no oauth token found for github.com` になった。Git 操作ができても GitHub API が使えるとは限らない。`gh auth login` で認証し、`gh auth status` で確認してから API を使う。

### #3 Ruleset は failure / success path まで確認する

静的設定だけでなく、`verify FAIL → merge BLOCKED` / `verify PASS → merge ALLOWED` まで実証して初めて、required check が実際の merge gate として機能していると言える。

## 4. 再現コマンド

```bash
gh auth status
export GITHUB_TOKEN="$(gh auth token)"

# 正本と live の比較
python3 scripts/github/verify_main_ruleset.py
python3 scripts/github/sync_main_ruleset.py --dry-run

# 契約チェックと品質ゲート
python3 scripts/check_ruleset_contract.py
make verify
```

## 5. 次タスクへの引き継ぎ

Task 22 固有の Ruleset 調整は完了。今後 Ruleset を変更する場合は以下の順で扱う。

1. `main-ruleset.json` を変更
2. `check_ruleset_contract.py`
3. `sync_main_ruleset.py --dry-run`
4. live / `bypass_actors` を確認
5. 必要なら適用
6. `verify_main_ruleset.py` で再確認
