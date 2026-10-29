#!/usr/bin/env bash
# Reproduce the Gate G4 measurements from docs/rust-rewrite-plan.md.
#
# Compares strategies A/B/D/E/F/G (0,1,3,4,5,6) against the upstream Festival
# 3.1 C++ solver on the smoke set and Microban I levels 1-80, and compares the
# reverse packing plans at a fixed tree size.
#
# Requires the upstream C++ checkout:
#   FESTIVAL_CPP_DIR (default: ../festival)
#
# Environment:
#   G4_TIME        per-level time budget in seconds (default 60)
#   G4_LEVELS      last Microban level (default 80)
#   G4_PLAN_LOG    log2 tree nodes for the plan comparison (default 20)
#   G4_SKIP_BUILD  set to 1 to reuse existing binaries
#   G4_SKIP_PLANS  set to 1 to reuse plan files already in target/g4/plans
set -euo pipefail

cd "$(dirname "$0")/.."

TIME="${G4_TIME:-60}"
LAST="${G4_LEVELS:-80}"
PLAN_LOG="${G4_PLAN_LOG:-20}"
STRATEGIES="0 1 3 4 5 6"

PYTHON="${PYTHON:-python3}"
if ! command -v "$PYTHON" >/dev/null 2>&1; then
    PYTHON=python
fi

if [ ! -f levels/corpus/microban.xsb ]; then
    scripts/fetch_corpus.sh
fi
sha256sum -c levels/corpus/microban.xsb.sha256

if [ "${G4_SKIP_BUILD:-0}" != "1" ]; then
    cargo build --release -q -p festival-cli -p festival-bench
    if [ ! -x target/festival-cpp.exe ]; then
        scripts/build_cpp_ref.sh
    fi
    scripts/build_refcheck.sh
fi

mkdir -p target/g4/plans

total_cpp_ms=0
total_rust_ms=0
summary="target/g4/summary.txt"
: > "$summary"

now_ms() {
    date +%s%N | cut -b1-13
}

parse_total() {
    sed -nE 's/.*OK: *([0-9]+) Failed: *([0-9]+) +([0-9]+) +([0-9]+).*/\1 \2 \3 \4/p' "$1"
}

run_solver() {
    local set="$1" levels_file="$2" alg="$3" last="$4"
    local cpp_dir="target/g4/$set-cpp$alg"
    local rust_dir="target/g4/$set-rust$alg"
    local range_cpp=() range_rust=()

    if [ -n "$last" ]; then
        range_cpp=(-from 1 -to "$last")
        range_rust=(--level "1..$last")
    fi

    mkdir -p "$cpp_dir" "$rust_dir"

    local start end
    start=$(now_ms)
    ./target/festival-cpp.exe "$levels_file" "${range_cpp[@]}" \
        -time "$TIME" -cores 1 -alg "$alg" -out_dir "$cpp_dir" > "$cpp_dir/run.log"
    end=$(now_ms)
    local cpp_ms=$((end - start))

    start=$(now_ms)
    ./target/release/festival-rs.exe "$levels_file" "${range_rust[@]}" \
        --time "$TIME" --cores 1 --alg "$alg" --out-dir "$rust_dir" > "$rust_dir/run.log"
    end=$(now_ms)
    local rust_ms=$((end - start))

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

    total_cpp_ms=$((total_cpp_ms + cpp_ms))
    total_rust_ms=$((total_rust_ms + rust_ms))

    printf '%-9s %s cpp %s/%s moves %s pushes %s time %sms | rust %s/%s moves %s pushes %s time %sms invalid %s\n' \
        "$set" "$alg" "$cpp_ok" "$((cpp_ok + cpp_failed))" "$cpp_moves" "$cpp_pushes" "$cpp_ms" \
        "$rust_ok" "$((rust_ok + rust_failed))" "$rust_moves" "$rust_pushes" "$rust_ms" "$invalid" \
        | tee -a "$summary"
}

echo "== packing plans (fixed tree log $PLAN_LOG, Microban 1-$LAST)"
plan_exact=0
plan_equivalent=0
plan_different=0
for alg in 0 1 3 4 5; do
    for level in $(seq 1 "$LAST"); do
        cpp_plan="target/g4/plans/cpp-$alg-$level.txt"
        rust_plan="target/g4/plans/rust-$alg-$level.txt"
        if [ "${G4_SKIP_PLANS:-0}" != "1" ] || [ ! -s "$cpp_plan" ] || [ ! -s "$rust_plan" ]; then
            ./target/refcheck.exe plan levels/corpus/microban.xsb "$level" "$alg" "$PLAN_LOG" > "$cpp_plan"
            ./target/release/festival-bench.exe packing-plan levels/corpus/microban.xsb \
                "$level" "$alg" "$PLAN_LOG" > "$rust_plan"
        fi
        verdict=$("$PYTHON" scripts/compare_packing_plans.py "$cpp_plan" "$rust_plan")
        case "$verdict" in
            exact*) plan_exact=$((plan_exact + 1)) ;;
            equivalent*)
                plan_equivalent=$((plan_equivalent + 1))
                echo "  alg $alg level $level: $verdict" | tee -a "$summary" ;;
            *)
                plan_different=$((plan_different + 1))
                echo "  alg $alg level $level: $verdict" | tee -a "$summary" ;;
        esac
    done
done
printf 'plans: %s exact, %s equivalent, %s different (of %s)\n' \
    "$plan_exact" "$plan_equivalent" "$plan_different" "$((5 * LAST))" | tee -a "$summary"

echo "== solver runs (time $TIME s per level)"
for alg in $STRATEGIES; do
    run_solver smoke levels/smoke/smoke.sok "$alg" ""
done
for alg in $STRATEGIES; do
    run_solver microban levels/corpus/microban.xsb "$alg" "$LAST"
done

printf 'total wall time: cpp %sms, rust %sms\n' "$total_cpp_ms" "$total_rust_ms" | tee -a "$summary"

if [ "$plan_different" -ne 0 ]; then
    echo "G4 FAIL: packing plans differ semantically" >&2
    exit 1
fi
echo "G4 plan comparison passed"
