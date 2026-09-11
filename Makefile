.PHONY: verify audit sqlx-prepare task-index install-git-hooks

# CodexはDATABASE_URLをコマンドラインで上書きしない。未設定時は安全なlocal DBを使う。
export DATABASE_URL ?= postgres://ops_hub:ops_hub@localhost:5433/ops_hub

# 人間 / Claude Code / Codex / 将来の品質CIで共有する唯一の最終品質ゲート。
verify:
	@python3 scripts/assert_local_database_url.py
	@python3 scripts/check_ruleset_contract.py
	@python3 scripts/run_quiet.py "AI tooling tests" -- python3 -m unittest discover -s scripts/tests -p 'test_*.py'
	@python3 scripts/run_quiet.py "cargo check" --cwd back_cargo --env SQLX_OFFLINE=true --unset-env DATABASE_URL -- cargo check --all-targets --all-features
	@python3 scripts/run_quiet.py "cargo fmt" --cwd back_cargo -- cargo fmt --all -- --check
	@python3 scripts/run_quiet.py "cargo clippy" --cwd back_cargo -- cargo clippy --all-targets --all-features -- -D warnings
	@python3 scripts/run_quiet.py "SQLx metadata" --cwd back_cargo -- cargo sqlx prepare --check -- --all-targets --all-features
	@python3 scripts/run_quiet.py "cargo test" --cwd back_cargo -- cargo test --all-targets --all-features
	@python3 scripts/run_quiet.py "frontend lint" -- npm run lint
	@python3 scripts/run_quiet.py "frontend build" -- npm run build
	@python3 scripts/check_task_docs.py
	@echo "make verify: PASS"

# 依存脆弱性は外部advisory DBに依存するため、実装品質ゲートとは分離する。
audit:
	@python3 scripts/run_quiet.py "cargo audit" --cwd back_cargo -- cargo audit

# query/migration変更後に .sqlx/ metadata を更新する。
# DATABASE_URL はローカル/CI PostgreSQLのみ許可する。
sqlx-prepare:
	@python3 scripts/assert_local_database_url.py
	cd back_cargo && cargo sqlx prepare -- --all-targets --all-features

# docs/task-INDEX.md を再生成する。
task-index:
	@python3 scripts/update_task_index.py

# 任意。Conventional Commits のローカルhookを有効化する。
install-git-hooks:
	git config core.hooksPath .githooks
	@echo "Git hooks enabled: .githooks/"
