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
| Phase 3+ | not started | full deadlock subsystem, packing/planning, GIRL/Dragonfly, multi-core scheduling |

The CLI now solves levels with a single-threaded **forward** FESS search
(`NORMAL` score). It writes `solutions.sok` and `times.txt` in `-out_dir` and
replays every produced LURD solution with the independent validator.

Phase 2 deliberately stops before reverse packing: `--forward-only` is the
supported mode, `-alg 0` is accepted as the forward part of strategy A, and
`-alg 1..7` is rejected with an explanatory error. Deadlock pruning uses the
static subset (embedded patterns, 2×2/3×3, freeze and fixed-box checks);
corral, matching, MPDB2, k-distance and wobblers arrive with Phase 3. Solution
quality is therefore not yet comparable to the C++ solver — FESS can return a
legal but non-minimal path until the planning features land in Phase 4.

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

Build the upstream C++ baseline for differential runs (requires a sibling
`festival` checkout, or set `FESTIVAL_CPP_DIR`):

```sh
scripts/build_cpp_ref.sh
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
  src/analysis/          rooms, hotspots, holes, biconnectivity, matching
  src/deadlock/          unified oracle, static patterns, 2x2/3x3/freeze
  src/search/            tree, heap, requests, scores, advisors, FESS loop
  src/io/lurd.rs         independent LURD replay validator
  src/io/solution.rs     player-path BFS, LURD and result files
crates/festival-cli/     festival-rs binary
crates/festival-bench/   static analysis CSV + solution verifier
levels/smoke/            12 self-authored regression levels
levels/corpus/           Microban I (155 levels, checksum + attribution)
reference/               C++ baseline notes, pthread shim, refcheck driver
scripts/                 corpus fetch, Gate G2, C++ baseline build
```

## Licensing

Festival 3.1 is copyright Yaron Shoham and is distributed with the request that
users credit the program and its author. This port keeps the original notices
and does **not** relicense the derived work. See [`NOTICE`](NOTICE);
the final project license is still to be confirmed with the original author.
