# festival-rs

Rust port of the Festival 3.1 Sokoban solver by Yaron Shoham.

The goal is a behavior-equivalent, memory-safe, cross-platform rewrite of the
FESS / GIRL / Dragonfly solver. This repository follows
[`docs/rust-rewrite-plan.md`](docs/rust-rewrite-plan.md); each phase has a Gate
before the next one starts.

## Current status

| Phase | Status | Contents |
|---|---|---|
| Phase 0 | complete | workspace, CLI, smoke set, LURD validator, CI |
| Phase 1 | complete | board, `.sok` parser, preprocessing, BFS, box graph, push/pull distances, rooms, hotspots, holes, biconnectivity, fixed boxes, Hungarian matching |
| Phase 2 | complete (forward-only, Gate G2) | arena tree, indexed heap, FESS requests/scores/advisors, unified deadlock oracle, cyclic `has_winning_move`, forward FESS loop, solution extraction, `solutions.sok` / `times.txt` |
| Phase 3 | complete (deadlock subsystem, Gate G3) | deadlock zone, corral/π-corral, positions arena, deadlock cache, advanced/matching/pull elimination, k-distance, XY, rooms, wobblers, mini search, MPDB2, stuck learning, cycle deadlock, envelope learning, C++ deadlock differential |
| Phase 4 | complete (packing/planning/HF, Gate G4) | reverse packing, park order, OOP, imagine/overlap, base matching, HF/BICON/MAX_DIST/REV scoring, strategies A/B/D/E/F/G end-to-end, packing-plan differential |
| Phase 5 | complete (GIRL/Dragonfly, Gate G5) | GIRL value function and ε-greedy tree descent, Dragonfly priority-queue pull search, NAIVE forward scoring, strategies C/H, GIRL score/value differential |
| Phase 6 | complete (scheduler/multicore, Gate G6) | SNAIL/NETLOCK detection and scoring, 8-strategy schedule, 1/2/4/8-core workers with shared perimeter and atomic cancellation, `--save-best`, XSokoban corpus, snail-state differential |
| Phase 7 | complete (performance/memory/stability, Gate G7; 24 h soak pending) | scratch/graph reuse, cross-level worker reuse, generation-cleared deadlock cache, C++-equivalent phase clock, rectangle-init single-queue BFS with direct component marking, `try_reserve` failure path, parser/LURD/move fuzzing, RSS soak harness |

The CLI runs the same reverse-packing → plan → forward-FESS structure as C++.
Without `--alg` it runs the eight-strategy schedule over 1/2/4/8 workers, like
C++; `--alg a`..`h` forces one strategy, and `--forward-only` keeps the Phase 2
NORMAL forward search. All eight strategies solve Microban 1–80 with the same
solve set as C++ (Gate G4). Strategy C (GIRL) has the same 78/80 solved set as
C++ and identical total moves/pushes; strategy H (Dragonfly + NAIVE) solves
80/80 with a 0.3% quality difference (Gate G5). With `--cores 1` the schedule
solves the same Microban 155 and XSokoban 1–20 sets as C++ (Gate G6), and
1/2/4/8-core runs match C++ on Microban 1–40. Every Rust solution replays
independently. Pinned single-core runtime is 0.56–0.57× (Microban 1–155) and
0.95–0.96× (XSokoban 1–20) of C++; paired 8-core Microban 1–155 finishes in
0.11× C++'s wall clock (cancellation-dominated, see `reference/BASELINE.md`),
and peak RSS is 0.22× (Gate G7 plus the worker-reuse and BFS follow-ups; the
24-hour soak is still pending).

`--cores` accepts 1/2/4/8 and defaults to the host processor count capped at 8.
Each worker owns its tree, helper and deadlock state; the perimeter and the
`any_core_solved` cancellation flag are shared. SNAIL/NETLOCK levels are
detected during preprocessing and switch strategies A/B/C to the corresponding
scoring modes with the C++ time allocation.

The deadlock subsystem remains the G3 implementation: 238,928 sampled boards
with 0 verdict mismatches and a cache-on/cache-off invariant. Output files are
`solutions.sok` and `times.txt` in `--out-dir`; every solution is replayed with
the independent validator.

## Build and run

```sh
cargo build --workspace
cargo test --workspace
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
```

Solve the bundled smoke levels:

```sh
cargo run -p festival-cli -- levels/smoke/smoke.sok --level 1 --time 60s --out-dir out
cargo run -p festival-cli -- levels/smoke/smoke.sok --max-expansions 500000 --out-dir out
```

Analyse levels without searching:

```sh
cargo run -p festival-cli -- levels/smoke/smoke.sok --level 1 --analyse --dump
cargo run -p festival-bench -- levels/smoke/smoke.sok
```

Reproduce the Gate G2 measurements (Microban I, 155 vendored levels):

```sh
scripts/run_g2.sh
cargo test -p festival-core --test corpus -- --ignored --nocapture
cargo run -p festival-bench -- verify-solutions levels/corpus/microban.xsb out/solutions.sok
```

Reproduce the Gate G3 deadlock differential (requires the C++ checkout):

```sh
PIN_CPU=1 scripts/run_g3.sh
```

`PIN_CPU` pins both binaries on hybrid CPUs (affinity mask on Windows, CPU
index on Linux); without it, P/E-core scheduling can move the timing ratio by
about 2x.

Build the upstream C++ baseline for differential runs (requires a sibling
`festival` checkout, or set `FESTIVAL_CPP_DIR`):

```sh
scripts/build_cpp_ref.sh
scripts/build_refcheck.sh
```

Reproduce Gate G4 (packing plans + strategies 0/1/3/4/5/6 on smoke and
Microban 1–80):

```sh
G4_TIME=60 scripts/run_g4.sh
```

Reproduce Gate G5 (strategies C/H, 100-level Dragonfly sweep, GIRL
score/value differential):

```sh
G5_TIME=60 scripts/run_g5.sh
```

Reproduce Gate G6 (snail detection/scoring differential, eight strategies, 1/2/4/8-core
schedules, Microban + XSokoban corpus):

```sh
G6_TIME=30 scripts/run_g6.sh
```

`G6_TIME` must be at least 24 s: C++ divides each strategy budget by three and
aborts on a zero-length HF phase on one core. The corpus step uses
`G6_CORPUS_TIME=120`, because XSokoban 14/16 and Microban 153 sit on the
time-limit boundary at 30–60 s.

Reproduce Gate G7 (toolchain gates, single-core runtime, 8-core throughput,
peak RSS, Miri, fuzzing, and a soak run with RSS sampling):

```sh
G7_PIN_CPU=1 G7_FUZZ_SECONDS=60 G7_SOAK_SECONDS=600 scripts/run_g7.sh
```

`G7_PIN_CPU=1` pins the single-core timing runs to one P-core; on this hybrid
CPU unpinned runs move between P- and E-cores and the ratio changes by up to
2x. The Gate criterion is a 24-hour soak: set `G7_SOAK_SECONDS=86400` for the
full run. `scripts/fuzz_env.sh` points `cargo fuzz` at LLVM's ASan runtime
because this Visual Studio install has no x64 AddressSanitizer component.

Baseline commands, recorded results and per-level quality differences are in
[`reference/BASELINE.md`](reference/BASELINE.md).

## Command line

```text
festival-rs <INPUT> [OPTIONS]
```

`INPUT` is a level set file, a name resolved as `<out-dir>/levels/<name>.sok`
when it contains no `/`, `\` or `.`, or `-` to read the level set from standard
input. Output files are `solutions.sok` and `times.txt` in `--out-dir`.

- `--level <SPEC>` selects levels, 1-based: `all` (the default), `3`, `1..5`,
  or `2,7,9`.
- `--time <DURATION>` sets the per-level budget. Units are `ms`, `s`, `m` and
  `h`; a bare number means seconds (`600` by default). `0` skips the search,
  which measures preprocessing and setup only.
- `--alg <STRATEGY>` forces one strategy, `a`..`h` or `0`..`7`. Without it the
  eight-strategy schedule runs. The forced choice is honored for every core
  count (C++ 3.1 ignores it when `--cores > 1`).
- `--cores <N>` accepts 1/2/4/8 and defaults to 1/2/4/8 rounded down from the
  available processors; C++ additionally checks RAM thresholds, which this port
  does not.
- `--format human|json` selects human readable output or one JSON document on
  standard output (`schema`, `command`, `results`, `summary`).
- `--out-file <FILE>` selects YASC mode: only the given solution file is
  written and `times.txt` is skipped.
- `--save-best` keeps the best partial solution when a level is not solved.
- `--max-expansions N` disables the wall-clock limit, which makes runs
  reproducible for differential testing.
- `--forward-only` skips reverse packing and runs the Phase 2 NORMAL search.
- `--analyse` prints static analysis instead of solving; add `--dump` for the
  board and room map.
- `--validate` replays every produced solution; `--tree-log-size N` and
  `--extra-mem N` control the tree capacity (default `log2(nodes) = 20`).

The C++-style single-dash options and `--from`/`--to` were removed; passing one
prints the replacement to use.

```sh
festival-rs levels/smoke/smoke.sok --level 1..3 --format json --out-dir out
cat levels/smoke/smoke.sok | festival-rs - --level 1 --time 30s
```

The default level set is `XSokoban`; it is not bundled.

## Layout

```text
crates/festival-core/    pure algorithms, #![forbid(unsafe_code)]
  src/board.rs           51x51 fixed board, C++ hash, rendering
  src/level/             .sok parsing and preprocess_level
  src/bfs.rs             connectivity BFS
  src/graph.rs           push/pull box-move graph
  src/distance.rs        distance matrices and impossible places
  src/analysis/          rooms, hotspots, holes, biconnectivity, matching, envelopes
  src/deadlock/          zone, corral, advanced/matching, positions, cache, k-dist,
                         XY, rooms, wobblers, mini, MPDB2, stuck, cycle
  src/search/            tree, heap, requests, scores, advisors, FESS loop,
                         packing search, HF/MAX_DIST scoring, GIRL/ε-greedy,
                         Dragonfly, NAIVE, SNAIL/NETLOCK, perimeter
  src/solver/            strategy A–H runner, 1/2/4/8-core scheduler, clocks
  src/planning/          park order, OOP, imagine/overlap, base matching, opener
  src/io/lurd.rs         independent LURD replay validator
  src/io/solution.rs     player-path BFS, LURD and result files
crates/festival-cli/     festival-rs binary
crates/festival-bench/   static analysis CSV, deadlock differential, packing plans,
                         solution verifier, soak driver
fuzz/                    cargo-fuzz targets for the parser, LURD replay and
                         move generation
levels/smoke/            12 self-authored regression levels
levels/corpus/           Microban I (155 levels) and XSokoban (90 levels),
                         checksums + attribution
reference/               C++ baseline notes, pthread/conio shims, refcheck driver
scripts/                 corpus fetch, Gates G2–G7, plan/score/solve-set
                         comparators, C++ builds, fuzz/ASan environment
```

## Licensing

Festival 3.1 is copyright Yaron Shoham and is distributed with the request that
users credit the program and its author. This port keeps the original notices
and does **not** relicense the derived work. See [`NOTICE`](NOTICE);
the final project license is still to be confirmed with the original author.
