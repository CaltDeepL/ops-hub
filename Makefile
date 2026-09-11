.PHONY: verify sqlx-prepare

verify:
	cargo fmt --all -- --check
	cargo clippy --all-targets --all-features -- -D warnings
	cargo sqlx prepare --check -- --all-targets --all-features
	cargo test --all-targets --all-features

sqlx-prepare:
	cargo sqlx prepare -- --all-targets --all-features