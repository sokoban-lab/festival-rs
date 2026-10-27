#!/usr/bin/env bash
# Reproduce the Gate G3 measurements from docs/rust-rewrite-plan.md.
#
# Requires the upstream C++ checkout used as the differential reference:
#   FESTIVAL_CPP_DIR (default: ../festival)
#
# On hybrid CPUs (Intel P/E cores) single-thread scheduling can shift the
# timings by ~2x. Set PIN_CPU to pin both sides: an affinity mask on Windows
# (PIN_CPU=1) or a CPU index on Linux (PIN_CPU=0).
set -euo pipefail

cd "$(dirname "$0")/.."

PIN_CPU="${PIN_CPU:-}"
pin() {
    if [ -z "$PIN_CPU" ]; then
        "$@"
    elif [ "$(uname -s)" = "Linux" ]; then
        taskset -c "$PIN_CPU" "$@"
    else
        cmd //c "start /affinity $PIN_CPU /wait /b $*"
    fi
}

if [ ! -f levels/corpus/microban.xsb ]; then
    scripts/fetch_corpus.sh
fi
sha256sum -c levels/corpus/microban.xsb.sha256

scripts/build_refcheck.sh

# 2000 boards per level; C++ additionally records per-call timings in .info.
mkdir -p target/g3
for n in $(seq 1 155); do
    pin target/refcheck.exe deadlock levels/corpus/microban.xsb "$n" 2000 50 7 \
        "target/g3/cpp-$n" >/dev/null
done

cargo build --release -q -p festival-bench

pin ./target/release/festival-bench.exe deadlock-diff \
    levels/corpus/microban.xsb target/g3 1 155 | tee target/g3/diff.log

pin ./target/release/festival-bench.exe deadlock-diff \
    levels/corpus/microban.xsb target/g3 1 155 --cache-off | tee target/g3/diff-cacheoff.log

# Solver regressions with the full deadlock subsystem enabled.
mkdir -p target/g3-smoke target/g3-microban
cargo run --release -q -p festival-cli -- levels/smoke/smoke.sok \
    --time 60 --cores 1 --forward-only --out_dir target/g3-smoke
cargo run --release -q -p festival-cli -- levels/corpus/microban.xsb \
    --from 1 --to 40 --time 60 --cores 1 --forward-only --out_dir target/g3-microban
./target/release/festival-bench.exe verify-solutions \
    levels/corpus/microban.xsb target/g3-microban/solutions.sok
