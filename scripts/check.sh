#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
cargo fmt -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --release --locked
"${CARGO_TARGET_DIR:-target}/release/linux-task-manager" --self-test
