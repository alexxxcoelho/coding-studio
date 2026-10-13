# Build artefacts are kept outside kDrive so the sync client never uploads them.
export CARGO_TARGET_DIR ?= $(HOME)/.cache/coding-studio/target

THESIS ?= $(abspath ../..)

# The React UI is edited in web/ (synced) but installed and built in a cache
# folder, so node_modules never enters kDrive. The built files are copied to
# crates/cs-app/web-dist and committed: `cargo build` needs no Node.
WEB_BUILD ?= $(HOME)/.cache/coding-studio/web

.PHONY: check desktop-check test lint fmt build run desktop desktop-app desktop-dev web web-dev web-test
# Everything that runs without GTK: the same subset as tools/check.sh.
check:
	cargo fmt --all -- --check
	cargo clippy --workspace --exclude cs-desktop --all-targets -- -D warnings
	cargo test --workspace --exclude cs-desktop
# Full workspace, including cs-desktop (needs GTK on Linux).
desktop-check: fmt lint test
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
# Packaged macOS app and disk image. Needs the Tauri CLI: cargo install tauri-cli --version "~2.12" --locked
desktop:
	cd crates/cs-desktop && cargo tauri build
	@echo "Bundle: $(CARGO_TARGET_DIR)/release/bundle/ (macos/*.app, dmg/*.dmg)"
# The .app only, without the disk image.
desktop-app:
	cd crates/cs-desktop && cargo tauri build --bundles app
	@echo "Bundle: $(CARGO_TARGET_DIR)/release/bundle/macos/"
# Native window on the Vite dev server (hot reload of web/). Run `pnpm --dir web install` once first.
desktop-dev:
	cd crates/cs-desktop && cargo tauri dev
web:
	mkdir -p "$(WEB_BUILD)"
	rsync -a --delete --exclude node_modules --exclude dist web/ "$(WEB_BUILD)/"
	cd "$(WEB_BUILD)" && pnpm install --frozen-lockfile && CS_GUIDE_DIR=$(abspath docs/guide) pnpm build
	rsync -a --delete "$(WEB_BUILD)/dist/" crates/cs-app/web-dist/
web-test:
	mkdir -p "$(WEB_BUILD)"
	rsync -a --delete --exclude node_modules --exclude dist web/ "$(WEB_BUILD)/"
	cd "$(WEB_BUILD)" && pnpm install --frozen-lockfile && CS_GUIDE_DIR=$(abspath docs/guide) pnpm exec vitest run && pnpm lint
web-dev:
	rsync -a --delete --exclude node_modules --exclude dist web/ "$(WEB_BUILD)/"
	cd "$(WEB_BUILD)" && pnpm install --frozen-lockfile && CS_GUIDE_DIR=$(abspath docs/guide) pnpm dev
