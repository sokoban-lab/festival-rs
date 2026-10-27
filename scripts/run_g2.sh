#!/usr/bin/env bash
# Reproduce the Gate G2 measurements from docs/rust-rewrite-plan.md.
set -euo pipefail

cd "$(dirname "$0")/.."

if [ ! -f levels/corpus/microban.xsb ]; then
    scripts/fetch_corpus.sh
fi
sha256sum -c levels/corpus/microban.xsb.sha256

mkdir -p target/g2
cargo run --release -q -p festival-cli -- levels/corpus/microban.xsb \
    --from 1 --to 40 --time 60 --cores 1 --alg 0 --out_dir target/g2

mkdir -p target/g2-no-panic
cargo run --release -q -p festival-cli -- levels/corpus/microban.xsb \
    --from 1 --to 100 --time 1 --cores 1 --alg 0 --out_dir target/g2-no-panic

cargo test --release -p festival-core --test corpus -- --ignored --nocapture
