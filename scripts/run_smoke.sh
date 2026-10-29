#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run --quiet -p festival-cli -- levels/smoke/smoke.sok --time 60 --analyse
cargo run --quiet -p festival-bench -- levels/smoke/smoke.sok
