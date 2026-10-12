#!/usr/bin/env bash
# Workspace check without cs-desktop (GTK is not available on headless hosts).
# Usage: tools/check.sh    (CARGO_TARGET_DIR may be set by the caller)
set -u
cd "$(dirname "$0")/.."
cargo fmt --all
cargo test --workspace --exclude cs-desktop 2>&1 | grep -E "test result|FAILED|^error" | sort | uniq -c
echo "clippy warnings: $(cargo clippy --workspace --exclude cs-desktop --all-targets 2>&1 | grep -c '^warning')"
