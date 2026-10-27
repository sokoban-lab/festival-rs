#!/usr/bin/env bash
# Reproduce the Gate G5 measurements from docs/rust-rewrite-plan.md.
#
# 1. Strategies C (GIRL) and H (Dragonfly + NAIVE) versus the upstream solver
#    on the smoke set and Microban I 1-80.
# 2. A 100-level Dragonfly sweep with independent solution replay.
# 3. GIRL push/pull score and value differential on sampled reachable boards
#    with an identical parking plan on both sides.
#
# Requires the upstream C++ checkout:
#   FESTIVAL_CPP_DIR (default: ../festival)
#
# Environment:
#   G5_TIME          per-level budget for the strategy comparison (default 60)
#   G5_LAST          last Microban level for comparisons (default 80)
#   G5_SWEEP_LAST    last level for the Dragonfly sweep (default 100)
#   G5_SWEEP_TIME    per-level budget for the sweep (default 10)
#   G5_BOARDS        sampled boards per level for the GIRL diff (default 300)
#   G5_SKIP_BUILD    set to 1 to reuse binaries
set -euo pipefail

cd "$(dirname "$0")/.."

TIME="${G5_TIME:-60}"
LAST="${G5_LAST:-80}"
SWEEP_LAST="${G5_SWEEP_LAST:-100}"
SWEEP_TIME="${G5_SWEEP_TIME:-10}"
BOARDS="${G5_BOARDS:-300}"

PYTHON="${PYTHON:-python3}"
if ! command -v "$PYTHON" >/dev/null 2>&1; then
    PYTHON=python
fi

if [ ! -f levels/corpus/microban.xsb ]; then
    scripts/fetch_corpus.sh
fi
sha256sum -c levels/corpus/microban.xsb.sha256

if [ "${G5_SKIP_BUILD:-0}" != "1" ]; then
    cargo build --release -q -p festival-cli -p festival-bench
    if [ ! -x target/festival-cpp.exe ]; then
        scripts/build_cpp_ref.sh
    fi
    scripts/build_refcheck.sh
fi

mkdir -p target/g5/plans target/g5/boards target/g5/girl
summary="target/g5/summary.txt"
: > "$summary"

now_ms() {
    date +%s%N | cut -b1-13
}

parse_total() {
    sed -nE 's/.*OK: *([0-9]+) Failed: *([0-9]+) +([0-9]+) +([0-9]+).*/\1 \2 \3 \4/p' "$1"
}

run_solver() {
    local set="$1" levels_file="$2" alg="$3" last="$4"
    local cpp_dir="target/g5/$set-cpp$alg"
    local rust_dir="target/g5/$set-rust$alg"
    local range_cpp=() range_rust=()

    if [ -n "$last" ]; then
        range_cpp=(-from 1 -to "$last")
        range_rust=(--from 1 --to "$last")
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
        --time "$TIME" --cores 1 --alg "$alg" --out_dir "$rust_dir" > "$rust_dir/run.log"
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

    printf '%-9s %s cpp %s/%s moves %s pushes %s time %sms | rust %s/%s moves %s pushes %s time %sms invalid %s\n' \
        "$set" "$alg" "$cpp_ok" "$((cpp_ok + cpp_failed))" "$cpp_moves" "$cpp_pushes" "$cpp_ms" \
        "$rust_ok" "$((rust_ok + rust_failed))" "$rust_moves" "$rust_pushes" "$rust_ms" "$invalid" \
        | tee -a "$summary"

    RUN_CPP_OK=$cpp_ok
    RUN_RUST_OK=$rust_ok
    RUN_INVALID=$invalid
}

echo "== strategy comparison (time $TIME s per level)" | tee -a "$summary"
for alg in 2 7; do
    run_solver smoke levels/smoke/smoke.sok "$alg" ""
    if [ "$RUN_INVALID" != "0" ]; then
        echo "G5 FAIL: invalid Rust solutions for smoke strategy $alg" >&2
        exit 1
    fi
done
for alg in 2 7; do
    run_solver microban levels/corpus/microban.xsb "$alg" "$LAST"
    if [ "$RUN_INVALID" != "0" ]; then
        echo "G5 FAIL: invalid Rust solutions for Microban strategy $alg" >&2
        exit 1
    fi
    if [ "$RUN_CPP_OK" -gt 0 ]; then
        ratio=$("$PYTHON" -c "print($RUN_RUST_OK / $RUN_CPP_OK)")
        echo "  strategy $alg solve ratio vs C++: $ratio" | tee -a "$summary"
        "$PYTHON" -c "import sys; sys.exit(0 if $RUN_RUST_OK / $RUN_CPP_OK >= 0.98 else 1)" || {
            echo "G5 FAIL: strategy $alg below 98% of C++ solves" >&2
            exit 1
        }
    fi
done

echo "== Dragonfly 100-level sweep (time $SWEEP_TIME s per level)" | tee -a "$summary"
sweep_dir=target/g5/sweep-rust7
mkdir -p "$sweep_dir"
start=$(now_ms)
./target/release/festival-rs.exe levels/corpus/microban.xsb \
    --from 1 --to "$SWEEP_LAST" --time "$SWEEP_TIME" --cores 1 --alg 7 \
    --out_dir "$sweep_dir" > "$sweep_dir/run.log" 2>&1
end=$(now_ms)
sweep_ms=$((end - start))
./target/release/festival-bench.exe verify-solutions levels/corpus/microban.xsb \
    "$sweep_dir/solutions.sok" > "$sweep_dir/verify.log" 2>&1
sweep_invalid=$(sed -nE 's/.*checked [0-9]+ solutions, ([0-9]+) invalid.*/\1/p' "$sweep_dir/verify.log")
sweep_total=$(grep -E "Total" "$sweep_dir/times.txt" | tail -1)
printf 'sweep %s levels: %s invalid %s time %sms\n' "$SWEEP_LAST" "$sweep_total" "${sweep_invalid:-unknown}" "$sweep_ms" \
    | tee -a "$summary"
if [ "${sweep_invalid:-1}" != "0" ]; then
    echo "G5 FAIL: invalid Dragonfly sweep solutions" >&2
    exit 1
fi

echo "== GIRL score/value differential (Microban 1-$LAST)" | tee -a "$summary"
girl_rows=0
girl_max_diff=0
girl_log="target/g5/girl-diff.log"
: > "$girl_log"
for level in $(seq 1 "$LAST"); do
    cpp_plan="target/g4/plans/cpp-1-$level.txt"
    if [ ! -s "$cpp_plan" ]; then
        cpp_plan="target/g5/plans/cpp-1-$level.txt"
        ./target/refcheck.exe plan levels/corpus/microban.xsb "$level" 1 20 > "$cpp_plan"
    fi

    boards="target/g3all/cpp-$level.boards"
    if [ ! -s "$boards" ]; then
        boards="target/g5/boards/cpp-$level.boards"
        ./target/refcheck.exe deadlock levels/corpus/microban.xsb "$level" "$BOARDS" 0 7 \
            "target/g5/boards/cpp-$level" >/dev/null
    fi

    ./target/refcheck.exe girl levels/corpus/microban.xsb "$level" "$cpp_plan" "$boards" \
        "target/g5/girl/cpp-$level.txt" >/dev/null
    ./target/release/festival-bench.exe girl-trace levels/corpus/microban.xsb "$level" \
        "$cpp_plan" "$boards" "target/g5/girl/rust-$level.txt"

    verdict=$("$PYTHON" scripts/compare_girl_scores.py \
        "target/g5/girl/cpp-$level.txt" "target/g5/girl/rust-$level.txt")
    echo "level $level $verdict" >> "$girl_log"
    case "$verdict" in
        ok:*)
            rows=$(echo "$verdict" | sed -E 's/ok: ([0-9]+) rows.*/\1/')
            diff=$(echo "$verdict" | sed -E 's/.*max \|value diff\| ([0-9.e+-]+)/\1/')
            girl_rows=$((girl_rows + rows))
            girl_max_diff=$("$PYTHON" -c "print(max($girl_max_diff, $diff))")
            ;;
        *)
            echo "G5 FAIL: level $level GIRL score diff: $verdict" >&2
            exit 1
            ;;
    esac
done
printf 'girl rows %s, max |value diff| %s (limit 1e-5)\n' "$girl_rows" "$girl_max_diff" \
    | tee -a "$summary"
"$PYTHON" -c "import sys; sys.exit(0 if $girl_max_diff < 1e-5 else 1)" || {
    echo "G5 FAIL: GIRL value difference above 1e-5" >&2
    exit 1
}

echo "G5 passed" | tee -a "$summary"
