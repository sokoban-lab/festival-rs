#!/usr/bin/env bash
# Reproduce the Gate G6 measurements from docs/rust-rewrite-plan.md.
#
# 1. SNAIL / NETLOCK detection state and score payloads versus refcheck on
#    both corpora and the self-authored chain levels.
# 2. Each of the eight strategies run alone (`-alg 0..7`, one core).
# 3. The 1/2/4/8-core strategy schedule compared with C++.
# 4. The full Microban corpus and an XSokoban subset solved with the schedule.
# 5. Independent replay of every Rust solution.
#
# Requires the upstream C++ checkout:
#   FESTIVAL_CPP_DIR (default: ../festival)
#
# Environment:
#   G6_TIME            per-level budget for schedules/corpus (default 30;
#                      C++ needs >= 24 s on one core so no phase budget is 0)
#   G6_STRATEGY_TIME   per-level budget for the `-alg` sweep (default 30)
#   G6_STRATEGY_LAST   last Microban level for the `-alg` sweep (default 20)
#   G6_CORE_LAST       last Microban level for the core comparison (default 40)
#   G6_CORPUS_TIME     per-level budget for the corpus comparison (default 120;
#                      XSokoban 14/16 and Microban 153 are timing-boundary
#                      levels at 30-60 s)
#   G6_XSOKOBAN_LAST   last XSokoban level for the corpus comparison (default 20)
#   G6_SKIP_BUILD      set to 1 to reuse binaries
#   G6_MIRI            set to 1 to run the Miri scheduler subset
set -euo pipefail

cd "$(dirname "$0")/.."

TIME="${G6_TIME:-30}"
STRAT_TIME="${G6_STRATEGY_TIME:-30}"
STRAT_LAST="${G6_STRATEGY_LAST:-20}"
CORE_LAST="${G6_CORE_LAST:-40}"
CORPUS_TIME="${G6_CORPUS_TIME:-120}"
XS_LAST="${G6_XSOKOBAN_LAST:-20}"
MICROBAN_LAST="${G6_MICROBAN_LAST:-155}"

PYTHON="${PYTHON:-python3}"
if ! command -v "$PYTHON" >/dev/null 2>&1; then
    PYTHON=python
fi

if [ "$TIME" -lt 24 ]; then
    echo "G6 FAIL: G6_TIME must be >= 24 s; C++ 1-core phases otherwise get 0 s" >&2
    exit 1
fi

if [ ! -f levels/corpus/microban.xsb ]; then
    scripts/fetch_corpus.sh
fi
sha256sum -c levels/corpus/microban.xsb.sha256
if [ ! -f levels/corpus/xsokoban.sok ]; then
    scripts/fetch_xsokoban.sh
fi
sha256sum -c levels/corpus/xsokoban.sok.sha256

if [ "${G6_SKIP_BUILD:-0}" != "1" ]; then
    cargo build --release -q -p festival-cli -p festival-bench
    if [ ! -x target/festival-cpp.exe ]; then
        scripts/build_cpp_ref.sh
    fi
    scripts/build_refcheck.sh
fi

mkdir -p target/g6
summary="target/g6/summary.txt"
: > "$summary"

parse_total() {
    sed -nE 's/.*OK: *([0-9]+) Failed: *([0-9]+) +([0-9]+) +([0-9]+).*/\1 \2 \3 \4/p' "$1"
}

run_solver() {
    local set="$1" levels_file="$2" alg="$3" cores="$4" seconds="$5"
    local first="$6" last="$7"
    local cpp_dir="target/g6/$set-cpp${alg:+-$alg}-c$cores"
    local rust_dir="target/g6/$set-rust${alg:+-$alg}-c$cores"
    local cpp_alg=() rust_alg=()

    if [ -n "$alg" ]; then
        cpp_alg=(-alg "$alg")
        rust_alg=(--alg "$alg")
    fi

    rm -rf "$cpp_dir" "$rust_dir"
    mkdir -p "$cpp_dir" "$rust_dir"

    ./target/festival-cpp.exe "$levels_file" -from "$first" -to "$last" \
        -time "$seconds" -cores "$cores" "${cpp_alg[@]}" -out_dir "$cpp_dir" \
        < /dev/null > "$cpp_dir/run.log"
    ./target/release/festival-rs.exe "$levels_file" --level "$first..$last" \
        --time "$seconds" --cores "$cores" "${rust_alg[@]}" --out-dir "$rust_dir" \
        > "$rust_dir/run.log"

    local cpp_total rust_total
    cpp_total=$(parse_total "$cpp_dir/times.txt")
    rust_total=$(parse_total "$rust_dir/times.txt")
    local cpp_ok cpp_failed cpp_moves cpp_pushes
    local rust_ok rust_failed rust_moves rust_pushes
    read -r cpp_ok cpp_failed cpp_moves cpp_pushes <<< "$cpp_total"
    read -r rust_ok rust_failed rust_moves rust_pushes <<< "$rust_total"

    ./target/release/festival-bench.exe verify-solutions "$levels_file" \
        "$rust_dir/solutions.sok" > "$rust_dir/verify.log" 2>&1
    local invalid
    invalid=$(sed -nE 's/.*checked [0-9]+ solutions, ([0-9]+) invalid.*/\1/p' "$rust_dir/verify.log")
    invalid="${invalid:-unknown}"

    printf '%-10s alg %-2s cores %s cpp %s/%s moves %s pushes %s | rust %s/%s moves %s pushes %s invalid %s\n' \
        "$set" "${alg:--}" "$cores" "$cpp_ok" "$((cpp_ok + cpp_failed))" "$cpp_moves" "$cpp_pushes" \
        "$rust_ok" "$((rust_ok + rust_failed))" "$rust_moves" "$rust_pushes" "$invalid" \
        | tee -a "$summary"

    if [ "$invalid" != "0" ]; then
        echo "G6 FAIL: invalid Rust solutions in $rust_dir" >&2
        exit 1
    fi

    CPP_DIR="$cpp_dir"
    RUST_DIR="$rust_dir"
}

check_sets() {
    local label="$1" tolerance="$2"
    local report
    if ! report=$("$PYTHON" scripts/compare_solved_sets.py \
        "$CPP_DIR/times.txt" "$RUST_DIR/times.txt" --tolerance "$tolerance"); then
        echo "$report" | tee -a "$summary"
        echo "G6 FAIL: solve-set difference above tolerance ($label)" >&2
        exit 1
    fi
    echo "$report" | tee -a "$summary"
}

echo "== SNAIL / NETLOCK detection differential" | tee -a "$summary"
snail_diffs=0
check_snail_level() {
    local file="$1" level="$2" tag="$3"
    ./target/refcheck.exe snail "$file" "$level" > "target/g6/snail-$tag-$level.cpp"
    ./target/release/festival-bench.exe snail-state "$file" "$level" > "target/g6/snail-$tag-$level.rust"
    if ! diff --strip-trailing-cr -q "target/g6/snail-$tag-$level.cpp" "target/g6/snail-$tag-$level.rust" >/dev/null; then
        echo "  snail mismatch: $tag level $level" | tee -a "$summary"
        snail_diffs=$((snail_diffs + 1))
    fi
}
for level in $(seq 1 "$MICROBAN_LAST"); do
    check_snail_level levels/corpus/microban.xsb "$level" microban
done
for level in $(seq 1 "$XS_LAST"); do
    check_snail_level levels/corpus/xsokoban.sok "$level" xsokoban
done
for level in 1 2 3; do
    check_snail_level levels/smoke/snail.sok "$level" chain
done
printf 'snail state: %s levels, %s mismatches\n' \
    "$((MICROBAN_LAST + XS_LAST + 3))" "$snail_diffs" | tee -a "$summary"
if [ "$snail_diffs" != "0" ]; then
    echo "G6 FAIL: snail detection mismatch" >&2
    exit 1
fi

echo "== eight strategies, one core (time $STRAT_TIME s, Microban 1-$STRAT_LAST)" | tee -a "$summary"
for alg in 0 1 2 3 4 5 6 7; do
    run_solver strategy levels/corpus/microban.xsb "$alg" 1 "$STRAT_TIME" 1 "$STRAT_LAST"
    check_sets "strategy $alg" 0
done

echo "== core schedule comparison (time $TIME s, Microban 1-$CORE_LAST)" | tee -a "$summary"
for cores in 1 2 4 8; do
    run_solver cores levels/corpus/microban.xsb "" "$cores" "$TIME" 1 "$CORE_LAST"
    check_sets "cores $cores" 0.01
done

echo "== corpus solve sets (time $CORPUS_TIME s, one core)" | tee -a "$summary"
run_solver corpus levels/corpus/microban.xsb "" 1 "$CORPUS_TIME" 1 "$MICROBAN_LAST"
check_sets "Microban 1-$MICROBAN_LAST" 0
run_solver corpus-xs levels/corpus/xsokoban.sok "" 1 "$CORPUS_TIME" 1 "$XS_LAST"
check_sets "XSokoban 1-$XS_LAST" 0

if [ "${G6_MIRI:-0}" = "1" ]; then
    echo "== Miri shared-state subset" | tee -a "$summary"
    cargo +nightly miri test -p festival-core --lib \
        perimeter::tests::concurrent_access_is_synchronized \
        > target/g6/miri.log 2>&1
    printf 'miri: passed (target/g6/miri.log)\n' | tee -a "$summary"
fi

echo "G6 passed" | tee -a "$summary"
