.PHONY: verify sqlx-prepare task-index install-git-hooks

# 人間 / Claude Code / Codex / 将来の品質CIで共有する唯一の最終品質ゲート。
verify:
	@python3 scripts/assert_local_database_url.py
	cd back_cargo && cargo fmt --all -- --check
	cd back_cargo && cargo clippy --all-targets --all-features -- -D warnings
	cd back_cargo && cargo sqlx prepare --check -- --all-targets --all-features
	cd back_cargo && cargo test --all-targets --all-features
	npm run lint
	npm run build
	@python3 scripts/check_task_docs.py

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
