# Build artefacts are kept outside kDrive so the sync client never uploads them.
export CARGO_TARGET_DIR ?= $(HOME)/.cache/coding-studio/target

THESIS ?= $(abspath ../..)

.PHONY: check test lint fmt build run desktop desktop-dev
check: fmt lint test
fmt:
	cargo fmt --all -- --check
lint:
	cargo clippy --workspace --all-targets -- -D warnings
test:
	cargo test --workspace
build:
	cargo build --release
run:
	cargo run --release -p cs-app -- \
	  --library "$(THESIS)/04_Fieldwork/Interviews" \
	  --db "$(THESIS)/05_Analysis/coding_studio/coding_studio.db" \
	  --coding-db "$(THESIS)/05_Analysis/coding/coding.db"
desktop:
	cd crates/cs-desktop && cargo tauri build
	@echo "Bundle: $(CARGO_TARGET_DIR)/release/bundle/"
desktop-dev:
	cargo run -p cs-desktop
