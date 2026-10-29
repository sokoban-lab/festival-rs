#!/usr/bin/env bash
# Reproduce the Gate G7 measurements from docs/rust-rewrite-plan.md.
#
# 1. Toolchain gates: fmt, clippy, tests, cargo-deny.
# 2. Single-core total runtime versus C++ on Microban 1-155 and XSokoban 1-20
#    (`G7_SINGLE_TIME`, repeated `G7_REPEATS` times, best wall clock wins).
# 3. 8-core throughput versus C++ on Microban 1-155 (`G7_CORE_TIME`).
# 4. Peak working set versus C++ for a memory-heavy XSokoban level with one
#    core and with eight cores.
# 5. Miri shared-state subset and optional fuzz sessions.
# 6. A soak run driven by `festival-bench soak` while this script samples the
#    process working set; a leak is a baseline creep above 20%.
#
# The full 24-hour acceptance run is:
#   G7_SOAK_SECONDS=86400 G7_RSS_TIME=5 scripts/run_g7.sh
# A shorter `G7_SOAK_SECONDS` records partial evidence only.
#
# Requires the upstream C++ checkout:
#   FESTIVAL_CPP_DIR (default: ../festival)
#
# Environment:
#   G7_SINGLE_TIME     per-level budget for the single-core comparison (60)
#   G7_REPEATS         timing repetitions, best run per solver (2)
#   G7_MICROBAN_LAST   last Microban level for the single-core pair (155)
#   G7_XSOKOBAN_LAST   last XSokoban level for the single-core pair (20)
#   G7_CORE_TIME       per-level budget for the 8-core comparison (60)
#   G7_CORE_LAST       last Microban level for the 8-core comparison (155)
#   G7_RSS_TIME        budget for the 8-core RSS sample (5)
#   G7_RSS_SINGLE_TIME budget for the 1-core RSS sample (30; C++ needs >= 24)
#   G7_RSS_LEVEL       XSokoban level used for RSS (16)
#   G7_SOAK_SECONDS    soak duration in seconds (600; 86400 for the Gate)
#   G7_SOAK_EXPANSIONS per-level expansion cap during the soak (3000)
#   G7_SOAK_CORES      worker threads during the soak (8)
#   G7_FUZZ_SECONDS    seconds per fuzz target (0 disables fuzzing)
#   G7_MIRI            set to 0 to skip the Miri subset
#   G7_REGRESSION      set to 1 to rerun the G3 and G6 differential scripts
#   G7_SKIP_BUILD      set to 1 to reuse binaries
#   G7_PIN_CPU         affinity mask for the single-core timing runs (empty =
#                      unpinned; on this hybrid host use 1 to select a P-core)
set -euo pipefail

cd "$(dirname "$0")/.."

CPP=target/festival-cpp.exe
RUST=target/release/festival-rs.exe
BENCH=target/release/festival-bench.exe
RSS_PS=scripts/process_rss.ps1

SINGLE_TIME="${G7_SINGLE_TIME:-60}"
REPEATS="${G7_REPEATS:-2}"
MICROBAN_LAST="${G7_MICROBAN_LAST:-155}"
XS_LAST="${G7_XSOKOBAN_LAST:-20}"
CORE_TIME="${G7_CORE_TIME:-60}"
CORE_LAST="${G7_CORE_LAST:-155}"
RSS_TIME="${G7_RSS_TIME:-5}"
RSS_SINGLE_TIME="${G7_RSS_SINGLE_TIME:-30}"
RSS_LEVEL="${G7_RSS_LEVEL:-16}"
SOAK_SECONDS="${G7_SOAK_SECONDS:-600}"
SOAK_EXPANSIONS="${G7_SOAK_EXPANSIONS:-3000}"
SOAK_CORES="${G7_SOAK_CORES:-8}"
FUZZ_SECONDS="${G7_FUZZ_SECONDS:-0}"
MIRI="${G7_MIRI:-1}"
SKIP_BUILD="${G7_SKIP_BUILD:-0}"
PIN_CPU="${G7_PIN_CPU:-}"

if [ "$SINGLE_TIME" -lt 24 ]; then
    echo "G7 FAIL: G7_SINGLE_TIME must be >= 24 s; C++ 1-core phases otherwise get 0 s" >&2
    exit 1
fi
if [ "$RSS_SINGLE_TIME" -lt 24 ]; then
    echo "G7 FAIL: G7_RSS_SINGLE_TIME must be >= 24 s for the C++ 1-core run" >&2
    exit 1
fi

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
if [ ! -f levels/corpus/xsokoban.sok ]; then
    scripts/fetch_xsokoban.sh
fi
sha256sum -c levels/corpus/xsokoban.sok.sha256

mkdir -p target/g7
summary=target/g7/summary.txt
: > "$summary"
failed=0

note() {
    echo "$*" | tee -a "$summary"
}

require_le() {
    local label="$1" value="$2" limit="$3"
    if awk -v v="$value" -v l="$limit" 'BEGIN { exit !(v <= l) }'; then
        note "  PASS $label: $value <= $limit"
    else
        note "  FAIL $label: $value > $limit"
        failed=1
    fi
}

now_ns() {
    date +%s%N
}

counts_of() {
    sed -nE 's/.*OK: *([0-9]+) Failed: *([0-9]+) +([0-9]+) +([0-9]+).*/\1 \2 \3 \4/p' "$1" | tail -1
}

# run_one <cpp|rust> <dir> <levels> <cores> <seconds> <first> <last>
# Echoes wall-clock milliseconds.
run_one() {
    local solver="$1" dir="$2" levels="$3" cores="$4" seconds="$5" first="$6" last="$7"
    local start end
    rm -rf "$dir"
    mkdir -p "$dir"
    start=$(now_ns)
    if [ "$solver" = cpp ]; then
        if [ "$cores" = "1" ]; then
            pin "$CPP" "$levels" -from "$first" -to "$last" -time "$seconds" \
                -cores "$cores" -out_dir "$dir" < /dev/null > "$dir/run.log"
        else
            "$CPP" "$levels" -from "$first" -to "$last" -time "$seconds" \
                -cores "$cores" -out_dir "$dir" < /dev/null > "$dir/run.log"
        fi
    else
        if [ "$cores" = "1" ]; then
            pin "$RUST" "$levels" --level "${first}..${last}" --time "$seconds" \
                --cores "$cores" --out-dir "$dir" > "$dir/run.log"
        else
            "$RUST" "$levels" --level "${first}..${last}" --time "$seconds" \
                --cores "$cores" --out-dir "$dir" > "$dir/run.log"
        fi
    fi
    end=$(now_ns)
    echo $(((end - start) / 1000000))
}

# measure_pair <label> <levels> <cores> <seconds> <first> <last>
measure_pair() {
    local label="$1" levels="$2" cores="$3" seconds="$4" first="$5" last="$6"
    local rep cpp_ms rust_ms best_cpp=0 best_rust=0
    local cpp_counts="" rust_counts="" best_cpp_counts="" best_rust_counts=""
    for rep in $(seq 1 "$REPEATS"); do
        cpp_ms=$(run_one cpp "target/g7/$label-cpp-$rep" "$levels" "$cores" "$seconds" "$first" "$last")
        cpp_counts=$(counts_of "target/g7/$label-cpp-$rep/times.txt")
        rust_ms=$(run_one rust "target/g7/$label-rust-$rep" "$levels" "$cores" "$seconds" "$first" "$last")
        rust_counts=$(counts_of "target/g7/$label-rust-$rep/times.txt")
        if [ "$rep" -eq 1 ] || [ "$cpp_ms" -lt "$best_cpp" ]; then
            best_cpp=$cpp_ms
            best_cpp_counts=$cpp_counts
        fi
        if [ "$rep" -eq 1 ] || [ "$rust_ms" -lt "$best_rust" ]; then
            best_rust=$rust_ms
            best_rust_counts=$rust_counts
        fi
        note "  $label rep $rep: cpp ${cpp_ms} ms [$cpp_counts] | rust ${rust_ms} ms [$rust_counts]"
    done
    local cpp_ok rust_ok
    cpp_ok=$(echo "$best_cpp_counts" | cut -d' ' -f1)
    rust_ok=$(echo "$best_rust_counts" | cut -d' ' -f1)
    if [ "$cpp_ok" != "$rust_ok" ]; then
        note "  FAIL $label solved count differs: cpp [$best_cpp_counts] rust [$best_rust_counts]"
        failed=1
    fi
    local ratio
    ratio=$(awk -v r="$best_rust" -v c="$best_cpp" 'BEGIN { printf "%.3f", r / c }')
    note "  $label best: cpp ${best_cpp} ms, rust ${best_rust} ms, ratio $ratio"
    MEASURED_RATIO="$ratio"
}

# rss_once <cpp|rust> <dir> <levels> <cores> <seconds> <first> <last>
# Echoes "<peak bytes> <exit code>".
rss_once() {
    local solver="$1" dir="$2" levels="$3" cores="$4" seconds="$5" first="$6" last="$7"
    local exe arglist result
    rm -rf "$dir"
    mkdir -p "$dir"
    if [ "$solver" = cpp ]; then
        exe="$CPP"
        arglist="$levels|-from|$first|-to|$last|-time|$seconds|-cores|$cores|-out_dir|$dir"
    else
        exe="$RUST"
        arglist="$levels|--level|${first}..${last}|--time|$seconds|--cores|$cores|--out-dir|$dir"
    fi
    result=$(powershell -NoProfile -ExecutionPolicy Bypass -File "$RSS_PS" \
        -Out "$dir/stdout.log" -Exe "$exe" -ArgList "$arglist" -Csv "$dir/rss.csv")
    echo "$result"
}

# measure_rss <label> <cores> <seconds>
measure_rss() {
    local label="$1" cores="$2" seconds="$3"
    local cpp_result rust_result cpp_peak cpp_code rust_peak rust_code
    cpp_result=$(rss_once cpp "target/g7/$label-cpp" levels/corpus/xsokoban.sok \
        "$cores" "$seconds" "$RSS_LEVEL" "$RSS_LEVEL")
    read -r cpp_peak cpp_code <<< "$cpp_result"
    rust_result=$(rss_once rust "target/g7/$label-rust" levels/corpus/xsokoban.sok \
        "$cores" "$seconds" "$RSS_LEVEL" "$RSS_LEVEL")
    read -r rust_peak rust_code <<< "$rust_result"

    if [ "$cpp_code" != "0" ] || [ "$rust_code" != "0" ]; then
        note "  FAIL $label exit codes: cpp $cpp_code rust $rust_code"
        failed=1
    fi

    local ratio cpp_mb rust_mb
    ratio=$(awk -v r="$rust_peak" -v c="$cpp_peak" 'BEGIN { printf "%.3f", r / c }')
    cpp_mb=$(awk -v b="$cpp_peak" 'BEGIN { printf "%.1f", b / 1048576 }')
    rust_mb=$(awk -v b="$rust_peak" 'BEGIN { printf "%.1f", b / 1048576 }')
    note "  $label XSokoban $RSS_LEVEL: cpp ${cpp_mb} MB, rust ${rust_mb} MB, ratio $ratio"
    MEASURED_RATIO="$ratio"
}

soak_peak_check() {
    local csv="$1"
    awk -F, 'NR > 1 { n = NR - 1; bytes[n] = $2 }
        END {
            # Working set is a sawtooth: every level allocates and frees its
            # tree. Compare the steady-state mean of the second quintile with
            # the last quintile, plus the half-over-half peak.
            q = int(n / 5)
            if (q < 1) q = 1
            s1 = 0; c1 = 0; s2 = 0; c2 = 0
            for (i = q + 1; i <= 2 * q && i <= n; i++) { s1 += bytes[i]; c1++ }
            for (i = n - q + 1; i <= n; i++) { s2 += bytes[i]; c2++ }
            p1 = 0; p2 = 0
            for (i = 1; i <= int(n / 2); i++) if (bytes[i] > p1) p1 = bytes[i]
            for (i = int(n / 2) + 1; i <= n; i++) if (bytes[i] > p2) p2 = bytes[i]
            mean1 = s1 / c1
            mean2 = s2 / c2
            printf "%.3f %.3f %.1f %.1f %.1f %.1f\n", mean2 / mean1, p2 / p1, \
                mean1 / 1048576, mean2 / 1048576, p1 / 1048576, p2 / 1048576
        }' "$csv"
}

echo "== toolchain gates" | tee -a "$summary"
cargo fmt --all --check 2>&1 | tee -a "$summary"
cargo clippy --workspace --all-targets -- -D warnings 2>&1 | tee -a "$summary"
cargo test --workspace 2>&1 | tee -a "$summary"
cargo deny check 2>&1 | tee -a "$summary"
note "  toolchain gates passed"

if [ "$SKIP_BUILD" != "1" ]; then
    cargo build --release -q -p festival-cli -p festival-bench
    if [ ! -x "$CPP" ]; then
        scripts/build_cpp_ref.sh
    fi
    scripts/build_refcheck.sh
fi

if [ "$MIRI" != "0" ]; then
    echo "== Miri shared-state subset" | tee -a "$summary"
    cargo +nightly miri test -p festival-core --lib \
        perimeter::tests::concurrent_access_is_synchronized \
        > target/g7/miri.log 2>&1
    note "  miri: passed (target/g7/miri.log)"
fi

echo "== single-core runtime (time ${SINGLE_TIME}s, best of ${REPEATS})" | tee -a "$summary"
measure_pair microban-single levels/corpus/microban.xsb 1 "$SINGLE_TIME" 1 "$MICROBAN_LAST"
require_le "Microban 1-$MICROBAN_LAST runtime ratio" "$MEASURED_RATIO" 1.25
measure_pair xsokoban-single levels/corpus/xsokoban.sok 1 "$SINGLE_TIME" 1 "$XS_LAST"
require_le "XSokoban 1-$XS_LAST runtime ratio" "$MEASURED_RATIO" 1.25

echo "== 8-core throughput (time ${CORE_TIME}s, best of ${REPEATS})" | tee -a "$summary"
measure_pair microban-eight levels/corpus/microban.xsb 8 "$CORE_TIME" 1 "$CORE_LAST"
require_le "Microban 1-$CORE_LAST 8-core runtime ratio" "$MEASURED_RATIO" 1.25

echo "== peak working set (XSokoban $RSS_LEVEL)" | tee -a "$summary"
measure_rss rss-one 1 "$RSS_SINGLE_TIME"
require_le "1-core peak RSS ratio" "$MEASURED_RATIO" 1.20
measure_rss rss-eight 8 "$RSS_TIME"
require_le "8-core peak RSS ratio" "$MEASURED_RATIO" 1.20

echo "== soak (${SOAK_SECONDS}s, ${SOAK_CORES} cores, ${SOAK_EXPANSIONS} expansions/level)" | tee -a "$summary"
soak_dir=target/g7/soak
rm -rf "$soak_dir"
mkdir -p "$soak_dir"
soak_result=$(powershell -NoProfile -ExecutionPolicy Bypass -File "$RSS_PS" \
    -Out "$soak_dir/stdout.log" -Exe "$BENCH" \
    -ArgList "soak|levels/corpus/microban.xsb|$SOAK_SECONDS|$SOAK_EXPANSIONS|$SOAK_CORES" \
    -Csv "$soak_dir/rss.csv" -IntervalMs 500)
read -r soak_peak soak_code <<< "$soak_result"
note "  soak exit $soak_code, peak $(awk -v b="$soak_peak" 'BEGIN { printf "%.1f", b / 1048576 }') MB"
tail -3 "$soak_dir/stdout.log" >> "$summary"
if [ "$soak_code" != "0" ] || ! grep -q '^soak done' "$soak_dir/stdout.log"; then
    note "  FAIL soak did not finish cleanly"
    failed=1
else
    read -r drift peak_drift first_mb last_mb first_peak last_peak \
        <<< "$(soak_peak_check "$soak_dir/rss.csv")"
    note "  soak RSS: mean $first_mb -> $last_mb MB (drift $drift), peak $first_peak -> $last_peak MB (drift $peak_drift)"
    require_le "soak RSS mean drift" "$drift" 1.20
    require_le "soak RSS peak drift" "$peak_drift" 1.20
fi

if [ "$FUZZ_SECONDS" -gt 0 ]; then
    echo "== fuzz (${FUZZ_SECONDS}s per target)" | tee -a "$summary"
    # shellcheck disable=SC1091
    . scripts/fuzz_env.sh
    for target in parser lurd moves; do
        (cd fuzz && cargo +nightly fuzz run "$target" -- -max_total_time="$FUZZ_SECONDS") \
            > "target/g7/fuzz-$target.log" 2>&1
        note "  fuzz $target: passed (target/g7/fuzz-$target.log)"
    done
fi

if [ "${G7_REGRESSION:-0}" = "1" ]; then
    echo "== differential regressions (G3, G6)" | tee -a "$summary"
    scripts/run_g3.sh
    G6_SKIP_BUILD=1 scripts/run_g6.sh
    note "  G3 and G6 passed"
fi

if [ "$failed" -ne 0 ]; then
    echo "G7 FAILED" | tee -a "$summary"
    exit 1
fi
echo "G7 passed" | tee -a "$summary"
