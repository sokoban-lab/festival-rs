# C++ baseline

Status: built and used for the Gate G2 differential run on 2026-10-27.

## Source

- Festival 3.1, commit `f7c023122f4e93e5e649ec856a9721b592d1108b`
  (`2025-10-04`, `feat: initial commit`)
- Local source directory: `D:\Users\sms\Desktop\Repos\festival`
- Author: Yaron Shoham (copyright 2018-2022)

## Toolchain

| Tool | Version | Notes |
|---|---|---|
| Rust | 1.99.1 | used for this port |
| clang++ | 23.1.4 | MSVC target; `reference/cpp-shim/pthread.h` supplies the missing pthread API |

Build with `scripts/build_cpp_ref.sh` (uses `FESTIVAL_CPP_DIR=../festival` by default), or directly:

```sh
cd ../festival
clang++ -O3 -DTHREADS -I ../festival-rs/reference/cpp-shim *.cpp \
  -o ../festival-rs/target/festival-cpp.exe
```

Do **not** add `-D_CRT_SECURE_NO_WARNINGS`: `global.h` maps that macro to
`VISUAL_STUDIO`, which changes the default level directory and tree sizes.

## Gate G2 differential (Microban I, first 40 levels)

Commands:

```sh
# C++ (from target/cpp-microban60)
target/festival-cpp.exe levels/corpus/microban.xsb \
  -from 1 -to 40 -time 60 -cores 1 -alg 0

# Rust (from repository root)
cargo run --release -q -p festival-cli -- levels/corpus/microban.xsb \
  --from 1 --to 40 --time 60 --cores 1 --alg 0 --out_dir target/g2-rust
```

> Phase 4 restored C++ compatibility: `--alg 0` now means strategy A. To
> reproduce the forward-only G2 numbers below, use `--forward-only` instead of
> `--alg 0`; `scripts/run_g2.sh` has been updated accordingly. The recorded G2
> run predates the strategy dispatcher.

| Metric | C++ | Rust | Ratio |
|---|---|---|---|
| Solved | 40/40 | 40/40 | same set |
| Moves | 2383 | 2343 | 98.3% |
| Pushes | 650 | 638 | 98.2% |
| Independent LURD replay | 40/40 valid | 40/40 valid | — |

The verifier is `festival-bench verify-solutions <levels> <solutions.sok>`;
it replays every listed solution from the initial board without using the
search tree. It was run on **both** solution files.

Per-level differences (C++ moves/pushes vs Rust moves/pushes):

```text
 4  51/13 vs 41/11    21 37/9  vs 19/5     35  91/31 vs 129/37
 7  36/6  vs 38/6     22 49/15 vs 47/15    36 224/67 vs 184/59
17  27/9  vs 29/9     26 42/10 vs 45/10    38  39/10 vs 43/14
30  29/7  vs 33/9     32 36/9  vs 35/9     40  36/11 vs 32/9
34  80/22 vs 62/14
```

The remaining levels are identical. Phase 2 has no packing/planning features,
so individual solutions can be longer; the aggregate stays inside the project
thresholds (pushes ≤102%, moves ≤105%).

Recorded checksums:

```text
3be52f27e1f1e325dee6bc68fd73a3013d93bdb7f7762cc2516f974beea4b4a1 *target/cpp-microban60/times.txt
3ddf536876df0f9844cbb1e1de5a08cdf33261162fdcfb0e6ffcc19cc99283e8 *target/cpp-microban60/solutions.sok
1a761a5d0ce91206dbed421ffa47bdb49c32983560a3cb12fd8a807aa77e9a5b *target/g2-rust/times.txt
b8d50898111416bcdf679189def23d4b49bd4b0420d5a15bc57fe26813d05f5b *target/g2-rust/solutions.sok
```

## Rust 100-level no-panic sweep

```sh
cargo run --release -q -p festival-cli -- levels/corpus/microban.xsb \
  --from 1 --to 100 --time 1 --cores 1 --alg 0 --out_dir target/g2-no-panic
```

Exit status 0, 96/100 solved at a 1 s budget, zero panics or bounds errors.
The ignored test `microban_first_100_no_panic` repeats the sweep through the
library API with a 250 ms budget (93/100 solved).

## Gate G3 deadlock differential (2026-10-27)

`reference/refcheck.cpp` samples reachable boards by walking the solver's pruned
macro moves plus every physically legal push/pull successor, then records
`board_is_deadlocked` (push and pull) and `move_is_deadlocked` for the first
`move_samples` boards of each level. `festival-bench deadlock-diff` replays the
boards through the Rust port and compares verdicts; with `--cache-off` it also
compares a run whose cache returns no memoized verdicts.

```sh
PIN_CPU=1 scripts/run_g3.sh
```

The i9-14900HX host has P/E cores; unpinned single-thread runs drifted between
0.9x and 2.0x because the scheduler moved between core types. Both binaries are
therefore pinned to logical CPU 0 (`start /affinity 1`) and the C++ timings are
recorded in the same pinned runs (`.info` files).

Sampling: all 155 Microban levels, up to 2000 boards per level, 50 move samples
per level, seed 7. Result: **238,928 boards and 67,626 move checks**.

| Metric | Cache on | Cache off |
|---|---|---|
| Board verdict mismatches | 0 | 0 |
| False positives (Rust deadlocked, C++ live) | 0 | 0 |
| False negatives (C++ deadlocked, Rust live) | 0 | 0 |
| Move verdict mismatches | 0 | 0 |
| Verdict differences vs cache-on | — | 0 |
| Rust/C++ board time per call | 1.00x | 1.12x |
| Rust/C++ move time per call | 1.00x | 1.09x |

C++ board time 18,305 ns/call, move time 34,591 ns/call. The first differential
passes exposed a real gap: `init_envelope_patterns` had not been ported, so
envelopes were empty and the fixed-box transform kept boxes that C++ wallifies.
The resulting matching deadlocks were all false negatives; after adding the
envelope learning (mini search based) the corpus is an exact match.

Solver regressions with the full deadlock subsystem (`-time 60 -cores 1 -alg 0`):

| Metric | C++ (G2 run) | Rust (G3 run) |
|---|---|---|
| Microban 1–40 solved | 40/40 | 40/40 |
| Moves / pushes | 2383 / 650 | 2359 / 640 |
| LURD replay | valid | 40/40 valid |

Smoke set: 12/12 solved and replayed. The Gate's strategy A/B solve-rate item
is a Phase 4 dependency: both strategies are built on reverse packing,
base matching and HF scoring, which are not part of Phase 3. The forward-only
`-alg 0` solve set matches C++ on these 40 levels.

Recorded checksums (artifacts of this run; `times.txt` embeds a timestamp and
the diff logs embed per-level timings, so they are identifiers rather than
bit-reproducible content):

```text
c1daf1555e73edfcc235639defe5b78e70946891e957f03397640bdfcd0d0721 *target/g3all/diff.log
8b9e3fbe760a041d25e732f9e95ffdbf8e143a715e8e5ff5330c3faf2172429d *target/g3all/diff-cacheoff.log
758594de76a9a403ce62e01a5771d28c65fd39cf0034ff0e87912204a2446e65 *target/g3-microban/solutions.sok
3b6cf6fd55e51791e7c8fbb63133ecd61eb8a9c3031f1ec183470abcf407ee6a *target/g3-microban/times.txt
```

## Gate G4 differential (Microban I, first 80 levels)

Commands (from repository root, same machine as G2/G3):

```sh
G4_TIME=60 scripts/run_g4.sh
```

The recorded run generated all 800 plan files with the full script; the solving
half was then repeated with `G4_SKIP_BUILD=1 G4_SKIP_PLANS=1` after the first
invocation was interrupted by an edit to the still-running script. The reuse
flags only skip regeneration; the solver runs, plan comparisons and checksums
below come from the completed invocation.

The script builds the Rust binaries, the upstream C++ solver and `refcheck`,
compares the packing plans, runs both solvers for strategies 0, 1, 3, 4, 5, 6
on the smoke set and Microban 1–80 (`-time 60 -cores 1`), and replays every
Rust solution. Runs are not CPU-pinned; the recorded times include process
startup and level preprocessing.

### Solver results

| Strategy | Set | C++ solved | Rust solved | C++ moves/pushes | Rust moves/pushes | Ratios (moves/pushes) | C++ time | Rust time |
|---|---|---|---|---|---|---|---|---|
| 0 A | smoke | 12/12 | 12/12 | 111 / 36 | 111 / 36 | 100.0% / 100.0% | 2.95 s | 1.82 s |
| 1 B | smoke | 12/12 | 12/12 | 231 / 76 | 231 / 76 | 100.0% / 100.0% | 2.70 s | 1.38 s |
| 3 D | smoke | 12/12 | 12/12 | 223 / 76 | 223 / 76 | 100.0% / 100.0% | 1.98 s | 0.68 s |
| 4 E | smoke | 12/12 | 12/12 | 223 / 76 | 223 / 76 | 100.0% / 100.0% | 1.76 s | 0.90 s |
| 5 F | smoke | 12/12 | 12/12 | 231 / 76 | 231 / 76 | 100.0% / 100.0% | 1.26 s | 0.67 s |
| 6 G | smoke | 12/12 | 12/12 | 185 / 60 | 185 / 60 | 100.0% / 100.0% | 1.08 s | 0.53 s |
| 0 A | Microban 1–80 | 80/80 | 80/80 | 6577 / 1745 | 6625 / 1763 | 100.7% / 101.0% | 6.41 s | 2.67 s |
| 1 B | Microban 1–80 | 80/80 | 80/80 | 6362 / 1691 | 6362 / 1691 | 100.0% / 100.0% | 7.38 s | 3.33 s |
| 3 D | Microban 1–80 | 80/80 | 80/80 | 6385 / 1719 | 6385 / 1719 | 100.0% / 100.0% | 7.99 s | 3.41 s |
| 4 E | Microban 1–80 | 80/80 | 80/80 | 6364 / 1707 | 6364 / 1707 | 100.0% / 100.0% | 6.73 s | 4.04 s |
| 5 F | Microban 1–80 | 80/80 | 80/80 | 6388 / 1713 | 6388 / 1713 | 100.0% / 100.0% | 8.73 s | 4.10 s |
| 6 G | Microban 1–80 | 80/80 | 80/80 | 6746 / 1789 | 6742 / 1793 | 99.9% / 100.2% | 11.89 s | 7.52 s |

- Solve sets are identical for every strategy; all 12 + 12 smoke and
  **480** Microban Rust solutions replay from the initial board with
  `festival-bench verify-solutions` (0 invalid).
- Strategy A pushes are 101.0% of C++, inside the 102% Gate limit.
- Total wall time: C++ 60.84 s, Rust 31.06 s, **0.51×** (Gate limit 1.5×).
  Every individual run is faster in Rust; the largest ratio is 0.63×.

Per-level differences (only strategies A and G differ; B/D/E/F are identical
level by level):

```text
A:  13 53/23 vs 59/23    36 224/67 vs 212/71   61 114/25 vs 100/21   65 166/43 vs 188/47   77 269/65 vs 317/69
    22 49/15 vs 47/15    48 73/14  vs 89/22    62 76/34  vs 74/34    80 184/44 vs 170/46
G:  13 57/23 vs 63/23    35 139/35 vs 115/33   36 244/79 vs 258/85
```

### Packing-order comparison

For each of strategies 0, 1, 3, 4, 5 and levels 1–80 (400 pairs), the reverse
plan is produced by:

```sh
target/refcheck.exe plan levels/corpus/microban.xsb <level> <strategy> 20
target/release/festival-bench.exe packing-plan levels/corpus/microban.xsb <level> <strategy> 20
```

The tree log size fixes the expansion budget at `2^17 = 131072` expansions;
C++ `packing_search` stops at `tree_nearly_full`, and Microban plans converge
far below the cap, so the comparison is deterministic. `refcheck plan` uses
the upstream `packing_search` unchanged; it only allocates the tree with the
requested log size. `scripts/compare_packing_plans.py` accepts two plans when:

- every `from` is `-2` (initial box), `-1` (sink/permanent), or an earlier
  `to`; every `until` points at the step that pulls from that target;
- both lists move the same initial boxes and end on the same final targets
  (intermediate waypoints may differ).

Result: **397 exact, 3 equivalent variants, 0 semantic differences**.

```text
strategy 1, level 12: 5 vs 5 steps; only the intermediate target differs
                      (8 vs 4); same initial boxes and final targets.
strategy 3, level 12: same variant as above.
strategy 3, level 61: 7 vs 9 steps; C++ parks finals {9,10,11,31};
                      Rust uses intermediates 14 and 25 but reaches the same
                      final set. Both plans pass the chain checks.
```

Recorded checksums (the solution files embed a timestamp, so these identify
the run rather than being bit-reproducible):

```text
88c7097d60146b58a6bfe70894ceb3babe9d37fadd8f2a20dbeb347d69657db3 *target/g4/summary.txt
* Rust Microban solutions
4de54ec8b81bca24849bebb1d22851e1d0a44052dbc121f96ed9ddb675f6b7e6 *target/g4/microban-rust0/solutions.sok
3419b5adb60dc082b631fbb078a418617221cd40a1429ca9cd2dd3b036008f4b *target/g4/microban-rust1/solutions.sok
eb47c0fc0edb6851e2b412c23d25a317532a212d223c22a971edcc84cfd30e32 *target/g4/microban-rust3/solutions.sok
1a68cfd57bd72231aeb4aab251f8543fa4f664c2b946e50a5a5caf4d28165ea4 *target/g4/microban-rust4/solutions.sok
be1c35d68719e06b596742bdfb3c2d169b508f570d89191c99ec0dedba67c67e *target/g4/microban-rust5/solutions.sok
e539b0c4e0e852b8d366671f72b54e724a38338e25df7829300e7c1aa9bce170 *target/g4/microban-rust6/solutions.sok
cf0ab50e56e1037f3ce31f52ea5386f7ecf5fdc51b1cd4dacb5cf734c8806fe3 *target/g4/smoke-rust0/solutions.sok
bdc473f5704cbfe53838f188ddaa12017f7fe68656ec2a95e59fd4b0e43df306 *target/g4/smoke-rust1/solutions.sok
fb85bf57cddcd711a4034765a1fed58526ce14357dc8fc683a7c9cbbe5c49f74 *target/g4/smoke-rust3/solutions.sok
63b6ea185f3d1f37fbd85cc0461831e563d8e45ce152b7ff6f5026d4d0e43088 *target/g4/smoke-rust4/solutions.sok
75215247ad8914e3982010b0640a7e2e5912852f60d189209a949d7d7fe17fb9 *target/g4/smoke-rust5/solutions.sok
885aa7db2cb94e9ee692931ea30a9b0ee313d0306a35305a9ce0681caa4b8374 *target/g4/smoke-rust6/solutions.sok
```

`scripts/build_refcheck.sh` links `refcheck` with an 8 MB stack: `plan` calls
`packing_search` directly and the advisor scratch arrays exceed the default
1 MB main-thread stack at -O3. Solver logic and the reference binary are
unchanged.

## Still missing

Phase 5/6 items: GIRL (strategy C) and Dragonfly (strategy H), the 8-strategy
multi-core scheduler, shared deadlock/perimeter state across threads, and
per-node trace comparison (`refcheck trace`). The current differential covers
per-board/per-move deadlock verdicts, packing plans, end-to-end solve results
and independent solution replay.
