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