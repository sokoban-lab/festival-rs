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
| Phase 6+ | not started | 8-strategy scheduler, 1/2/4/8-core execution, shared deadlock/perimeter state |

The CLI runs the same reverse-packing → plan → forward-FESS structure as C++.
`-alg 0` means strategy A (C++ compatible), and `--forward-only` keeps the
Phase 2 NORMAL forward search. All eight strategies are available. Strategies
0/1/3/4/5/6 solve Microban 1–80 with the same solve set as C++ (Gate G4).
Strategy C (GIRL) has the same 78/80 solved set as C++ and identical total
moves/pushes; strategy H (Dragonfly + NAIVE) solves 80/80 with a 0.3% quality
difference (Gate G5). Every Rust solution replays independently. Multi-core
scheduling remains Phase 6, so `-cores > 1` still runs single-threaded.

The deadlock subsystem remains the G3 implementation: 238,928 sampled boards
with 0 verdict mismatches and a cache-on/cache-off invariant. Output files are
`solutions.sok` and `times.txt` in `-out_dir`; every solution is replayed with
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
cargo run -p festival-cli -- levels/smoke/smoke.sok -level 1 -time 60 -out_dir out
cargo run -p festival-cli -- levels/smoke/smoke.sok --max-expansions 500000 -out_dir out
```

Analyse levels without searching:

```sh
cargo run -p festival-cli -- levels/smoke/smoke.sok -level 1 --analyse --dump
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

Baseline commands, recorded results and per-level quality differences are in
[`reference/BASELINE.md`](reference/BASELINE.md).

Original single-dash options (`-level`, `-from`, `-to`, `-out_dir`, `-time`,
`-out_file`, `-save_best`, `-cores`, `-alg`, `-extra_mem`) and the new
`--max-expansions`, `--forward-only`, `--validate`, `--analyse`,
`--tree-log-size` options are accepted. `-out_file` selects YASC mode: only the
given solution file is written and `times.txt` is skipped.

`--max-expansions N` disables the wall-clock limit, which makes runs
reproducible for differential testing.

A level argument containing `/`, `\` or `.` is used as a path. Otherwise it is
resolved as `<out_dir>/levels/<name>.sok`, matching the C++ behavior. The
default level set is `XSokoban`; it is not bundled.

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
                         Dragonfly, NAIVE, perimeter
  src/solver/            strategy A–H runner, clocks, outcome
  src/planning/          park order, OOP, imagine/overlap, base matching, opener
  src/io/lurd.rs         independent LURD replay validator
  src/io/solution.rs     player-path BFS, LURD and result files
crates/festival-cli/     festival-rs binary
crates/festival-bench/   static analysis CSV, deadlock differential, packing plans,
                         solution verifier
levels/smoke/            12 self-authored regression levels
levels/corpus/           Microban I (155 levels, checksum + attribution)
reference/               C++ baseline notes, pthread shim, refcheck driver
scripts/                 corpus fetch, Gates G2/G3/G4/G5, plan/score comparators,
                         C++ builds
```

## Licensing

Festival 3.1 is copyright Yaron Shoham and is distributed with the request that
users credit the program and its author. This port keeps the original notices
and does **not** relicense the derived work. See [`NOTICE`](NOTICE);
the final project license is still to be confirmed with the original author.
