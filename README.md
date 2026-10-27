# festival-rs

Rust port of the Festival 3.1 Sokoban solver by Yaron Shoham.

The goal is a behavior-equivalent, memory-safe, cross-platform rewrite of the
FESS / GIRL / Dragonfly solver. This repository follows
[`docs/rust-rewrite-plan.md`](docs/rust-rewrite-plan.md); each phase has a Gate
before the next one starts.

## Current status

| Phase | Status | Contents |
|---|---|---|
| Phase 0 | partial | workspace, CLI skeleton, smoke set, LURD validator, CI |
| Phase 1 | in progress | board, `.sok` parser, preprocessing, BFS, box graph, push/pull distances, rooms, hotspots, holes, biconnectivity |
| Phase 2+ | not started | FESS tree/engine, deadlock subsystem, packing/planning, GIRL/Dragonfly, scheduling |

The CLI currently runs static analysis only. The search engine arrives in
Phase 2, so `-alg`, `-out_file`, `-save_best` and `--forward-only` are parsed and
reported as not implemented instead of silently doing something else.

## Build and run

```sh
cargo build --workspace
cargo test --workspace
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
```

Analyse the bundled smoke levels:

```sh
cargo run -p festival-cli -- levels/smoke/smoke.sok -level 1 --dump
cargo run -p festival-bench -- levels/smoke/smoke.sok
```

Original single-dash options (`-level`, `-from`, `-to`, `-out_dir`, `-time`,
`-out_file`, `-save_best`, `-cores`, `-alg`, `-extra_mem`) and the new
`--max-expansions`, `--forward-only`, `--validate` options are accepted.

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
  src/analysis/          rooms, hotspots, holes, biconnectivity
  src/io/lurd.rs         independent LURD replay validator
crates/festival-cli/     festival-rs binary
crates/festival-bench/   static analysis CSV driver
levels/smoke/            12 self-authored regression levels
reference/               C++ baseline notes and future refcheck driver
```

## Licensing

Festival 3.1 is copyright Yaron Shoham and is distributed with the request that
users credit the program and its author. This port keeps the original notices
and does **not** relicense the derived work. See [`NOTICE`](NOTICE);
the final project license is still to be confirmed with the original author.
