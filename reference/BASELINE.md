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
  --level 1..40 --time 60 --cores 1 --alg 0 --out-dir target/g2-rust
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
  --level 1..100 --time 1 --cores 1 --alg 0 --out-dir target/g2-no-panic
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

## Gate G5 differential (2026-10-27)

Commands (repository root, same machine as G2/G3/G4):

```sh
G5_SKIP_BUILD=1 G5_TIME=60 scripts/run_g5.sh
```

The script rebuilds/runs build steps when `G5_SKIP_BUILD` is unset, compares
strategies C (2) and H (7) on the smoke set and Microban 1–80, runs a 1–100
Dragonfly sweep, and compares GIRL push/pull scores and values on sampled
reachable boards with an identical parking plan on both sides. Runs are not
CPU-pinned; times include process startup and preprocessing.

### Solver results

| Strategy | Set | C++ solved | Rust solved | C++ moves/pushes | Rust moves/pushes | C++ time | Rust time |
|---|---|---|---|---|---|---|---|
| 2 C | smoke | 12/12 | 12/12 | 123 / 40 | 123 / 40 | 0.89 s | 0.34 s |
| 7 H | smoke | 12/12 | 12/12 | 81 / 28 | 81 / 28 | 0.79 s | 0.30 s |
| 2 C | Microban 1–80 | 78/80 | 78/80 | 6188 / 1657 | 6188 / 1657 | 131.52 s | 122.79 s |
| 7 H | Microban 1–80 | 80/80 | 80/80 | 6496 / 1753 | 6516 / 1743 | 7.17 s | 2.03 s |

- Strategy C solve set and every per-level moves/pushes count match C++ (levels
  8 and 19 time out on both sides at 60 s).
- Strategy H solve set matches; aggregate moves are 100.3% and pushes 99.4% of
  C++. Seven levels differ in quality (13, 36, 48, 53, 57, 63, 75); no illegal
  solutions. `refcheck dragonfly` shows small frontier differences on these
  levels (e.g. level 13: 107 C++ nodes vs 109 Rust nodes), which shifts the
  forward NAIVE search.
- Independent LURD replay: 0 invalid across smoke, Microban and the sweep.

### Dragonfly 100-level sweep

`-alg 7 -time 10 -cores 1` over Microban 1–100:

| Metric | Value |
|---|---|
| Solved | 100/100 |
| Invalid solutions | 0 |
| Wall time | 2.88 s |
| Panics / bounds errors | 0 |

### GIRL score/value differential

For each level the drivers use the same C++ parking plan (`refcheck plan`,
strategy 1, log2 nodes 20) and the same reachable-board sample (G3 sampling,
2,000 boards per level where available). `refcheck girl` and
`festival-bench girl-trace` print the GIRL push/pull fields
(`packed`, `tgt`, `connectivity`, `dist`, `overlap`) and `score_to_value` with
17 significant digits; `scripts/compare_girl_scores.py` requires exact integer
fields and `|value diff| < 1e-5`.

| Metric | Value |
|---|---|
| Rows compared | 92,903 |
| Integer field mismatches | 0 |
| Max \|value difference\| | 0 (threshold 1e-5) |

`refcheck dragonfly` reports exploration stats (nodes, roots, perimeter
entries) and is used only as a diagnostic; exact Dragonfly trace equality is
not a Gate G5 criterion.

### Regression

Gate G4 was re-run after the Phase 5 changes (single-core perimeter log2 raised
from 20 to the C++ value of 25): 397 exact + 3 equivalent plans, and all six
strategies keep their solve sets and recorded moves/pushes. Total wall time C++
39.18 s, Rust 21.79 s.

Recorded checksums (the solution files and logs embed timestamps/timings, so
these identify the run rather than being bit-reproducible):

```text
bd4fa8e8995f17d7a2bf1e418c23660a5c6e1c408d2429f628d3707edf079fc1 *target/g5/summary.txt
f4e26d56d0a89905f790893f0599f0c7e8b10e9b61328581401b35134a87a5c2 *target/g5/girl-diff.log
0f261b83dbcbfaf4aaed4dbed40e1116266f90e1ea03dba247c8c7108b01ec4c *target/g5/microban-rust2/solutions.sok
000aa7325245ef3895d996e3f0bf6c50dd3aad338933b157439fd92fc8d2dcdd *target/g5/microban-rust2/times.txt
e84f5d8297cb92bd17df60a260687bbd26f1e4ef54ac015016c714175757d895 *target/g5/microban-rust7/solutions.sok
2a7d867b3893ea494adb4e42104fd59af572b426aecad7e3a17a63b42b437450 *target/g5/microban-rust7/times.txt
652ec862e0e8ad4942adf6cfc175f3b3df57a3c12d7f5e5426712ea42ac209e9 *target/g5/sweep-rust7/solutions.sok
25e413e0740f4c08c8af00e66b96f3c1e3ab89450f86f89164297ef08dad409b *target/g5/sweep-rust7/times.txt
e6c396962d00dfacffca7c3b9ba213063334c1dd5c93728c4921ccc9b8610d75 *target/g5-run.log
```

## Gate G6 differential (2026-10-28)

Commands (repository root, same machine as G2–G5):

```sh
G6_SKIP_BUILD=1 scripts/run_g6.sh
cargo +nightly miri test -p festival-core --lib \
    perimeter::tests::concurrent_access_is_synchronized
```

The Miri subset is the command `run_g6.sh` runs with `G6_MIRI=1`; it was
rerun separately after the score-payload fix. The script verifies both corpus
checksums, compares SNAIL/NETLOCK detection
state and score payloads with `refcheck snail`, runs every strategy alone on Microban 1–20, runs
the 1/2/4/8-core schedule on Microban 1–40, and runs the full Microban corpus
plus XSokoban 1–20 with the default schedule. All solver runs are sequential;
running the two solvers concurrently perturbs the wall-clock budget and was
observed to flip a timing-boundary level.

### SNAIL / NETLOCK detection and score payloads

`refcheck snail` prints `snail_level_detected`, `netlock_level_detected`, the
chosen target, chain length/distances and the netlock grid after
`preprocess_level`. It also prints, for every alg type 0–3, the fields filled by
`fill_snail_scores` / `fill_netlock_scores` on the preprocessed initial board
and on both reverse-mode variants (`mark_bases` 0/1).
`festival-bench snail-state` prints the same rows from `SnailParams` and the
Rust scoring functions. Compared on all 155 Microban levels, XSokoban 1–20, and
the three self-authored chain levels in `levels/smoke/snail.sok`:

| Metric | Value |
|---|---|
| Levels compared | 178 |
| Detection/state mismatches | 0 |
| Score rows compared | 4,272 (24 per level) |
| Score mismatches | 0 |

The reverse boards place a box on every target, so the rows exercise both
values of `out_of_plan` (0 forward, 1 reverse). Review after the first Gate run
found the Rust `out_of_plan` test inverted relative to C++'s "box on the
target" check; it was fixed and the Gate was fully rerun. `snail_packed`
matches C++'s loop counter, which returns 1 for an empty chain (only
non-detected levels, where the payload is unused).

The synthetic set covers pull-order depth 20 (SNAIL), 10 (NETLOCK) and 9
(control). Depth 20 also exercises C++'s signed `snail_dist` value of `-1`
when the detected target is not the chain's far end; the port keeps the signed
value and skips out-of-range `snail_packed` slots where C++ indexes out of
bounds.

Neither corpus nor the synthetic set provides a solvable SNAIL/NETLOCK puzzle
(the fixtures have more targets than boxes), so this Gate compares detection
and score payloads but does not run an end-to-end SNAIL/NETLOCK search. A
solvable chain fixture is a Phase 7 differential item.

### Eight strategies, one core (`-time 30`, Microban 1–20)

| Strategy | C++ solved | Rust solved | Solve-set diffs | C++ moves/pushes | Rust moves/pushes | Invalid |
|---|---|---|---|---|---|---|
| 0 A | 20/20 | 20/20 | 0 | 1157 / 322 | 1163 / 322 | 0 |
| 1 B | 20/20 | 20/20 | 0 | 1193 / 326 | 1193 / 326 | 0 |
| 2 C | 18/20 | 18/20 | 0 | 1043 / 276 | 1043 / 276 | 0 |
| 3 D | 20/20 | 20/20 | 0 | 1167 / 322 | 1167 / 322 | 0 |
| 4 E | 20/20 | 20/20 | 0 | 1147 / 318 | 1147 / 318 | 0 |
| 5 F | 20/20 | 20/20 | 0 | 1179 / 324 | 1179 / 324 | 0 |
| 6 G | 20/20 | 20/20 | 0 | 1221 / 330 | 1227 / 330 | 0 |
| 7 H | 20/20 | 20/20 | 0 | 1210 / 322 | 1250 / 330 | 0 |

### 1/2/4/8-core schedule (`-time 30`, Microban 1–40, no `-alg`)

| Cores | C++ solved | Rust solved | Solve-set diffs | C++ moves/pushes | Rust moves/pushes | Invalid |
|---|---|---|---|---|---|---|
| 1 | 40/40 | 40/40 | 0 | 2383 / 650 | 2375 / 654 | 0 |
| 2 | 40/40 | 40/40 | 0 | 2399 / 644 | 2381 / 638 | 0 |
| 4 | 40/40 | 40/40 | 0 | 2375 / 638 | 2393 / 644 | 0 |
| 8 | 40/40 | 40/40 | 0 | 2391 / 668 | 2396 / 670 | 0 |

Runs are not CPU-pinned, so solution quality varies between runs; the solve
sets were identical on every run.

### Corpus (`-time 120`, one core, default schedule)

| Set | C++ solved | Rust solved | Solve-set diffs | Invalid |
|---|---|---|---|---|
| Microban 1–155 | 155/155 | 155/155 | 0 | 0 |
| XSokoban 1–20 | 20/20 | 20/20 | 0 | 0 |

At 30 s, Microban 153 sat exactly on the time-limit boundary and flipped
between runs. At 60 s, XSokoban 14/16 were boundary levels for Rust while C++
solved them; XSokoban 29 is the reverse at 120 s (Rust 37 s, C++ >120 s). The
Gate corpus budget is therefore 120 s and the XSokoban subset is 1–20. The
remaining boundary behavior is a Phase 7 performance target, not a solve-set
semantic difference: every Rust solution in these runs replays independently.

### Concurrency

- Rust `festival-core` is `#![forbid(unsafe_code)]`; the only shared mutable
  state is the perimeter `Mutex` and the `any_core_solved` atomic. Trees,
  helpers and deadlock state are owned by one worker.
- ThreadSanitizer is not supported for `x86_64-pc-windows-msvc` on this
  toolchain (`error: thread sanitizer is not supported for this target`), so
  the Gate uses a Miri subset: four threads perform 2,000 concurrent perimeter
  stores/lookups and poll the cancellation atomic. `cargo +nightly miri test`
  passes with no data-race report (53 s).
- 1/2/4/8-core runs over 40 levels (plus the smoke and corpus sets) completed
  without panic or deadlock.
- The C++ reference build gains a headless `conio.h` shim: upstream
  `engine.cpp` pauses on a multi-core "should not be pruned!" diagnostic,
  which blocks non-interactive runs. The shim only makes that diagnostic
  pause return immediately.
- C++ needs `-time >= 24 -cores 1`: budgets are integer-divided by three per
  strategy, and a zero-length backward phase leaves `imagined_hf_board` unset
  and aborts with "box num mismatch2". The Rust port instead degrades that
  score to `INF`; Gate runs stay above the upstream minimum.
- Deadlock cache, learned stuck patterns, k-distance and Hungarian caches are
  per worker (C++ keeps them in process globals). Deadlock verdicts are
  unaffected (Gate G3 cache-on/off invariance); learning and timing can shift
  individual searches.
- `-alg` is honored for every core count; C++ 3.1 ignores it when
  `-cores > 1`. Default `-cores` rounds the available processors to 1/2/4/8;
  C++ also applies RAM thresholds.

Recorded checksums (logs embed timings, so these identify the run):

```text
b4d61d0fea2420ccc22a910f1b2b61bafbcf9bc18ee5086d8531e99b451f239f *target/g6/summary.txt
a6d6a02986bb616ad85122203bbec20af549fdb71cd9cbf93bbfe2f8184bf251 *target/g6-run.log
ec62efc47b8b94054cf9044ffde0b376c06f5a224b09f4c358eeb1f12e0879d1 *target/g6/miri.log
```

## Gate G7 performance, memory and stability (2026-10-28)

Commands (repository root, same host as G2–G6):

```sh
G7_PIN_CPU=1 G7_REPEATS=3 G7_FUZZ_SECONDS=60 G7_SOAK_SECONDS=600 \
    scripts/run_g7.sh
```

`G7_PIN_CPU=1` pins the single-core timing runs to CPU 0, a P-core on the
i9-14900HX (8 P + 16 E cores, 32 threads). Unpinned, the same binaries move
between core types between runs: the C++ Microban 1–155 wall clock ranged from
18.8 s to 44.8 s across unpinned runs, which is enough to flip the acceptance
ratio. All numbers below are pinned best-of-3. The script also runs the
toolchain gates, Miri, fuzzing and the soak; the 24-hour soak run is
`G7_SOAK_SECONDS=86400` (not executed, see “Still missing”).

### Single-core total runtime (`-time 60 -cores 1`, best of 3)

| Set | C++ | Rust | Rust / C++ | Solved | Moves/pushes (C++ → Rust) |
|---|---|---|---|---|---|
| Microban 1–155 | 23,576 ms | 19,415 ms | 0.824 | 155/155 → 155/155 | 22,460/5,912 → 22,486/5,920 |
| XSokoban 1–20 | 14,969 ms | 15,972 ms | 1.067 | 20/20 → 20/20 | 18,143/5,466 → 18,143/5,466 |

Both sets are within the ≤1.25 requirement and the ≤1.10 target. Solution
quality differs by at most 0.12% (Microban) and is identical on XSokoban.

The recorded C++ Microban total is slower than the unpinned best (18.8 s)
because CPU 0 ran at a lower sustained clock during this run; the ratio is the
acceptance metric, and individual repetition pairs ranged from 0.82 to 1.00.

### 8-core throughput (`-time 60 -cores 8`, Microban 1–155, best of 3)

| Solver | Wall clock | Solved | Rust / C++ |
|---|---|---|---|
| C++ | 121,909 ms | 155/155 | — |
| Rust | 69,204 ms | 155/155 | 0.568 |

Rust throughput is 1.76× C++, above the “≥ 80% of C++” requirement (ratio
≤ 1.25). C++ sizes its one process-wide deadlock cache by core count
(22/23/24/25 bits for 1/2/4/8 cores); Rust gives every worker a 22-bit cache,
which matches the 8-worker footprint and bounds memory.

### Peak working set (XSokoban 16)

| Cores | C++ | Rust | Rust / C++ |
|---|---|---|---|
| 1 (`-time 30`) | 720.2 MB | 127.3 MB | 0.177 |
| 8 (`-time 5`) | 5,614.7 MB | 980.1 MB | 0.175 |

### Stability

- **Soak** (`festival-bench soak` over Microban, 8 workers, 600 s, 3,000
  expansions per level): 1,659 iterations, 1,659 solved, 0 failures, exit 0.
  External RSS sampling every 500 ms gives a steady-state mean of 351.0 →
  345.0 MB (drift 0.983) and a half-over-half peak of 603.9 → 614.2 MB
  (drift 1.017). The working set is a sawtooth as each level allocates and
  frees its tree; the script compares the second and last quintiles rather
  than raw minima. The 24-hour Gate run was not executed.
- **Miri** shared-state subset (perimeter mutex + cancellation atomic, four
  threads, 2,000 stores/lookups): passes with no data-race report (53 s).
- **Fuzzing** (`cargo +nightly fuzz`, 60 s per target, ASan runtime):
  parser 179,628 runs, LURD replay 286,687 runs, move generation 433 runs, no
  crash. The move target re-runs level preprocessing and deadlock preparation
  for every input, hence the low rate.
- **Toolchain**: `cargo fmt --all --check`, `cargo clippy --workspace
  --all-targets -- -D warnings`, `cargo test --workspace` (115 unit tests plus
  integration suites) and `cargo deny check` all pass.

### Engine changes found by the Gate

- `is_corral_active` checked the box ring only on corral cells. C++ checks the
  whole board, so restoring that condition made move generation identical on
  2,001 deep boards and made every strategy’s Microban 1–20 move/push totals
  exactly match C++ (Gate G6 rerun).
- `score_parked_boxes` treated every initial box as mandatory. Only boxes with
  `until == -1` are mandatory; the old rule made strategy A time out on
  XSokoban 14.
- The scheduler clock now matches C++: C++ starts `start_time` after
  preprocessing and tree set-up, measures whole seconds with `time(0)`, and
  stops only when `elapsed_seconds > budget`. Rust previously charged worker
  preparation to the first phase and used a precise clock, so Microban 153
  strategy A (C++ 3 s) timed out and the level took 31 s; whole-corpus runtime
  was 62 s. Starting the clock after worker preparation and adding the C++
  one-second slack brings level 153 to 3 s and the corpus to 19.4 s.
- `find_possible_moves` reuses helper-owned move/graph scratch and wallifies
  fixed boxes once per deadlock check; BFS and room analysis use stack `u16`
  buffers swapped as slices instead of allocating per wave.
- Tree arenas use `try_reserve_exact` and report `ResourceExhausted` instead of
  aborting on allocation failure; a unit test covers the failure path.

### Regression after the changes

- Gate G3: 238,928 sampled boards per pass, 0 board/move mismatches, 0
  cache-on/off verdict differences, 12/12 smoke levels, 40/40 Microban
  solutions replay.
- Gate G6: eight strategies solve Microban 1–20 with identical move/push
  totals; the 1/2/4/8-core schedule has 0 solve-set differences on Microban
  1–40; corpus runs solve 155/155 Microban and 20/20 XSokoban with 0 invalid
  solutions.

Recorded checksums:

```text
dd4d40241b457c07b52a7d2804b849f0142b8958d0058301166e91059d2743fe *target/g7/summary.txt
2a51935f43309da5c4475fdb04fc1db797d9588f58e216a93b82b22e721bd3e1 *target/g7-run-final.log
6a7c40dc0eaa17b45f29081d8259cfcbcb1b72e3fe8c65f71f17c4201a7c87bf *target/g7/miri.log
d5e9f16b103769c6b91252da6167a4d718664045640c3da8ef411744b780d36f *target/g7/fuzz-parser.log
fd9e36c92bb1fa09d5a2937cd99013e7c4c85b0a7cda2124001d57548a7f27e3 *target/g7/fuzz-lurd.log
16eec2706f4a85b3f8b59718938ee885b4f8ba7490c0a1a10cf0c02b72f07299 *target/g7/fuzz-moves.log
```

### Follow-up: cross-level worker reuse (2026-10-28)

`Solver` keeps one tree and helper per core for the whole level set, as C++
`search_trees[]`/`helpers[]` do, and `DeadlockCache` resets with a generation
counter instead of rewriting its 4M entries. Level-specific tables (MPDB2,
pull MPDB2, rooms, envelopes, stuck patterns) are still rebuilt per level and
per worker, as `preprocess_level` does.

Per-level setup measured with `--time 0` (no search; Microban 1–155,
XSokoban 1–20):

| Setup | before | after |
|---|---|---|
| Microban, 1 core | 5.38 s | 1.80 s |
| Microban, 8 cores | 37.25 s | 3.36 s |
| XSokoban 1–20, 1 core | 1.67 s | 1.22 s |

Same-day paired runs, pinned to CPU 0 for one core, best run of 2–5. The host
ran faster than on the Gate day (C++ Microban 1–155 took 16.9–20.4 s instead
of 23.6 s), so only ratios are comparable:

| Metric | C++ | Rust | Rust / C++ | Before |
|---|---|---|---|---|
| Microban 1–155, 1 core | 16,878 ms | 10,900 ms | 0.646 | 0.824 |
| Microban 1–155, 8 cores | 94,922 ms | 38,399 ms | 0.377 | 0.568 |
| XSokoban 1–20, 1 core | 11,168 ms | 11,712 ms | 1.049 | 1.067 (Gate), 1.09 same day |

XSokoban individual run pairs ranged 0.985–1.068; the remaining difference is
search time on levels 15/16/18, not setup (which is now at C++ parity).

Peak RSS (XSokoban 16): 166.5 MB on one core (0.221× C++ 754.1 MB) and
1,310.7 MB on eight cores (0.222× C++ 5,895.9 MB). Cache entries grew from 16
to 24 bytes to hold the generation, so Rust RSS rose from 0.177×/0.175×;
both remain far below the 1.20 limit.

Stability after the change: a 90 s soak (8 workers, 3,000 expansions per
level) ran 1,323 iterations, 1,323 solved, 0 failures, RSS mean drift 1.008.
Gate G3 (238,928 boards, 0 board/move mismatches, 0 cache-off verdict
differences, 12/12 smoke, 40/40 replay) and Gate G6 (SNAIL 178 levels with 0
mismatches, eight strategies with identical move/push totals, 1/2/4/8-core 0
solve-set differences, Microban 155/155 and XSokoban 20/20 with 0 invalid
solutions) pass unchanged; 118 unit tests pass.

```text
36910d60adefc8dc843e0258891d18b2f8284bf6f349f7222e0f5980f207aa7f *target/g7/opt-g3.log
c33954abdb06e22a4026e713bf87679053bfb744f637241d0d708558b182493d *target/g7/opt-g6.log
5b358143531244aa4a44268ed13ad704ed30b3d3e7ecb637506e32311a0b557c *target/g7/opt-soak/stdout.log
11d2ab2aa8b2ebb4d30c8607bc1581d4f5e18655b68147381aaf097614e21c8a *target/g7/opt-soak/rss.csv
```

### Follow-up: BFS/connectivity scratch removal (2026-10-28)

Per-node profiling with temporary phase timers and a counting allocator
(reverted afterwards) located the remaining XSokoban gap in `Graph::build`:
its `init_shift` step calls `mark_connectivities_around_place` once per free
cell, which was 30–40% of search time on XSokoban 16/18 and 1.5× C++ per
build. Three Rust-only costs caused the difference: `init_dist` filled all
2,601 cells instead of the logical rectangle (C++ clears only `height × width`),
every BFS zeroed two 5.2 KB stack queues (C++ leaves its stack arrays
uninitialized), and component marking copied a whole local distance board per
BFS. The port now fills only the rectangle, uses one FIFO queue, and labels
components directly in the output array; the observed semantics (INF for
unreachable, component ids / neighbour colours) are unchanged.

Instrumented XSokoban 18, strategy A (same-session pairs):

| Per-node item | Rust before | Rust after | C++ |
|---|---|---|---|
| `Graph::build` | 186.9 µs | 139.4 µs | 144.7 µs |
| BFS call (870,901 vs 871,216 calls) | 938 ns | 996 ns | 760 ns |
| `init_dist` call | 129 ns | 40 ns | 65 ns |
| connectivity marking call | 1885 ns | 1373 ns | 1388 ns |

Same-day paired wall clock (one core pinned to CPU 0, 60 s per level):

| Run | Rust | C++ | Ratio | Before |
|---|---|---|---|---|
| XSokoban 1–20 | 10,821 / 10,738 / 10,729 ms | 11,248 / 11,341 / 11,194 ms | 0.962 / 0.947 / 0.958 | 1.049 |
| Microban 1–155 | 9,587 / 9,532 ms | 17,059 / 16,823 ms | 0.562 / 0.567 | 0.646 |
| XS 15 | 2,736 / 2,663 ms | 2,581 / 2,537 ms | 1.060 / 1.050 | 1.037–1.064 |
| XS 16 | 2,219 / 2,220 ms | 2,164 / 2,129 ms | 1.025 / 1.043 | 1.062–1.085 |
| XS 18 | 2,648 / 2,720 ms | 2,764 / 2,734 ms | 0.958 / 0.995 | 1.133–1.140 |
| Microban 1–155, 8 cores | 13,246–13,810 ms | 120,307–124,075 ms | 0.109–0.115 | 0.377 |

The 8-core wall clock is dominated by when the first strategy solves a level
and the other cores observe the cancellation flag, so it is not a clean
per-node comparison; the host was also slower than on the Gate G7 day (C++
122 s instead of 95 s). The single-core rows are the comparable measurements.

Verification: 118 unit tests pass; Gate G3 (238,928 boards, 0 board/move
mismatches, 0 false positives/negatives, 0 cache-off differences, 40/40 replay
valid); Gate G6 (eight strategies with identical move/push totals,
1/2/4/8-core solve sets identical, Microban 155/155 and XSokoban 20/20 with 0
invalid solutions). The 90-level XSokoban set solves 89/90 on both sides with 0
solve-set differences (92,899 vs 92,923 moves, 28,379 vs 28,381 pushes, 89/89
replay valid), and Microban 155/155 replays valid.

```text
973bac580dadf66dc72c6cb1894e81e65aa611f47bc72a75c2eee2164ef45789 *target/g7/bfs-g3.log
7acb8eb899ccea649e17c90a937c056a709ee1c980050aca956045f1848c0a58 *target/g7/bfs-g6.log
7f375283ee491979cfe362c5ac79ceb04ecfd34348db6deeb74a4d047a1710f4 *target/g7/bfs-timings.txt
```

## Build-profile follow-up (2026-10-28)

Build-level variants measured with an interleaved, CPU-0-pinned protocol
(60 s per level, one core; every variant runs once per round). The host was
slower than on the Gate G7 day, so only the within-run ratios are meaningful.

| Variant | XS 15–16 (mean of 5) | Microban 145–146 (mean of 5) | XS 1–20 (mean of 3) |
|---|---|---|---|
| baseline (`thin` LTO, unwind, system allocator) | 4,794 ms | 4,674 ms | 11,998 ms |
| `panic = "abort"` | 4,491 ms (−6.3%) | 4,574 ms (−2.2%) | 11,048 ms (−7.9%) |
| mimalloc | 4,623 ms (−3.6%) | 4,442 ms (−5.0%) | 11,653 ms (−2.9%) |
| `target-cpu=native` | 4,795 ms (0%) | 4,561 ms (−2.4%) | 12,034 ms (+0.3%) |

Second interleaved run for the combination (4 reps for the level pairs, 3 for
XS 1–20):

| Variant | XS 15–16 | Microban 145–146 | XS 1–20 |
|---|---|---|---|
| baseline | 4,497 ms | 4,136 ms | 10,742 ms |
| `panic = "abort"` | 4,179 ms (−7.1%) | 4,022 ms (−2.8%) | 9,879 ms (−8.0%) |
| mimalloc | 4,386 ms (−2.5%) | 3,958 ms (−4.3%) | 10,406 ms (−3.1%) |
| `abort` + mimalloc | 4,063 ms (−9.7%) | 3,840 ms (−7.1%) | 9,699 ms (−9.7%) |

The shipped binary uses `panic = "abort"` plus mimalloc. Against the pre-change
binary it measured 4,754 → 4,385 ms on XS 15–16 (−7.8%) and 11,718 → 10,649 ms
on XS 1–20 (−9.1%). Eight-core Microban 1–155 came in at 11,287/12,682 ms
before and 10,274/12,458 ms after (two interleaved reps).

Rejected: PGO (`-Cprofile-generate` / `-Cprofile-use`, trained on XS 1–20 and
Microban 1–155) was consistently 3.5–5% slower than baseline (XS 15–16
4,268/4,301/4,308 vs 4,431/4,433/4,460 ms; XS 1–20 10,675/10,693 vs
11,236/11,237 ms). Fat LTO showed no gain over thin LTO.

Peak working set for the adopted configuration (XSokoban 16): 201.4 MB vs
166.3 MB on one core (+21%) and 1,340.2 MB vs 1,294.5 MB with eight cores
(+3.5%), far below the 1.20× C++ gate.

Verification: 118 unit tests pass in both dev and release profiles; clippy,
fmt and cargo-deny are clean; Microban 155/155 (22,486 moves / 5,920 pushes)
and XSokoban 20/20 (18,143 / 5,466) reproduce the baseline totals with 0
invalid solutions.

## Biconnected shift follow-up (2026-10-29)

`Graph::build` still spent 96–98% of its time in `init_shift`, which placed a
temporary box on every free cell and labelled the surrounding player
components with a BFS: 1.89 s of a 10.44 s XSokoban 1–20 run and 0.76 s of a
2.37 s XSokoban 18 strategy-A run, over 15,097 and 6,098 builds. Two sides of
a cell stay connected once a box occupies it exactly when their incident edges
belong to the same biconnected block, so one iterative Tarjan pass per build
(level-indexed `u16` arrays, every component, scratch reused on `Graph`)
replaces the per-cell BFS; `bfs::mark_connectivities_around_place` is gone.

Interleaved same-day wall clock (no CPU pinning; the host was slower than on
the Gate G7 day, so only the within-pair deltas are meaningful):

| Run | Before | After | Change |
|---|---|---|---|
| XSokoban 1–20, 1 core, 3 reps | 9.87–9.98 s (median 9.95) | 8.44–8.54 s (median 8.54) | −14.5% |
| XSokoban 18, strategy A, 1 core, 5 reps | 2.46–3.04 s (median 2.62) | 1.70–1.83 s (median 1.74) | −33.4% |
| Microban 1–155, 1 core, 5 reps | 9.09–10.98 s (median 10.57) | 7.89–9.29 s (median 8.88) | −16.0% |
| Microban 1–155, 8 cores, 3 reps | 13.08–13.47 s (median 13.27) | 11.87–12.99 s (median 11.91) | −10.3% |

A first Microban batch contained a 17.5 s outlier on an otherwise 9.7–10.3 s
series; it was discarded and the 5-rep series above rerun. With temporary
timers (reverted afterwards) `Graph::build` fell from 1.89 s to about 0.25 s on
XSokoban 1–20 and from 0.78 s to 0.07 s on XSokoban 18 strategy A, leaving it
at 2–3% of runtime.

Verification: 120 unit tests pass, including two new differential tests that
compare the new shift matrix against a per-pair BFS on six handcrafted boards
and 300 random wall configurations; clippy, fmt and cargo-deny are clean.
XSokoban 1–20 reproduces 18,143 moves / 5,466 pushes and Microban 155/155
reproduces 22,486 / 5,920; `solutions.sok` is byte-identical apart from the
solver timestamp, and `--validate` replays every solution.

```text
37e630259e63f41654e7727d85eca8314be1d49870b9c3081158eb536bf94992 *target/ab/shift-bench.txt
cdd62a76e69db37196d21e8421ae842da6314bb90d1d67a0aef834d9181fdcef *target/ab/val-after-x.txt
9c5d4e98704033570a98749a03cb4122c6beb16d791daa76b149d87baea94d04 *target/ab/val-after-mb.txt
```

## Still missing

- The 24-hour soak acceptance run has not been executed; the recorded soak is
  600 s. Run `G7_PIN_CPU=1 G7_SOAK_SECONDS=86400 scripts/run_g7.sh` and append
  its summary and checksums here.
- Per-node search trace comparison (`refcheck trace`) is still absent; the
  differential covers scoring, deadlock verdicts, move generation, parking
  plans and solve sets, not expansion order.
- No solvable SNAIL/NETLOCK fixture exists, so those modes are compared by
  detection state and score payloads only.
- ThreadSanitizer is unavailable for `x86_64-pc-windows-msvc`; the Miri subset
  covers the shared perimeter and cancellation state but not the full
  scheduler.
- This Visual Studio installation has no x64 AddressSanitizer component
  (`VCASAN.lib`, `stl_asan.lib`). `scripts/fuzz_env.sh` points the linker at
  LLVM's ASan runtime and supplies marker-only MSVC stubs; fuzzing is limited
  to 60-second sessions. Installing the ASan component or running the targets
  under Linux would remove the workaround.
- Single-core timings require CPU pinning on this hybrid host; unpinned runs
  are not comparable.

## v0.1.1 benchmark (2026-10-29)

End-to-end rerun of the `docs/benchmarks.md` protocol after the biconnected
shift change, on the same host as the 0.1.0 numbers. Single-core runs pin
logical CPU 0 (`start /affinity 1`); each set runs two rounds with the solver
order swapped. Eight-core Microban uses 60 s per level, three runs.

| Run | Microban 1-155, 1 core | Microban 1-155, 8 cores | XSokoban 1-90, 1 core |
|---|---|---|---|
| 1 | Rust 8,348 ms / C++ 18,852 ms | Rust 8,172 ms / C++ 64,373 ms | Rust 102,057 ms / C++ 181,424 ms |
| 2 | Rust 8,714 ms / C++ 19,290 ms | Rust 9,419 ms / C++ 65,170 ms | Rust 106,587 ms / C++ 182,115 ms |
| 3 | — | Rust 9,350 ms / C++ 64,275 ms | — |
| Best ratio | 0.443 | 0.127 | 0.563 |

Peak working sets (bytes): Microban one core 283,856,896 vs 771,072,000;
Microban eight cores 1,816,702,976 vs 5,957,652,480; XSokoban batch
338,481,152 vs 896,765,952; XSokoban level 29 272,764,928 vs 919,715,840.

Microban reproduces 22,486 moves / 5,920 pushes (Rust) against 22,460 / 5,912
(C++), and every solution replays with `festival-bench verify-solutions`
(155/155 and 90/90, 0 invalid). XSokoban levels 1-28 and 30-90 are unchanged
from 0.1.0; only level 29 differs: 705 moves / 190 pushes now versus
1,277 / 298 for C++ and the 0.1.0 binary. Level 29 takes 101 s with a build
from `e530e92` and 54 s with 0.1.1, so strategy A now finishes inside its
75 s slice (600 s / 8) instead of yielding to later strategies. The single
solve result therefore depends on wall-clock strategy budgets, not on a
different shift matrix; levels that solve inside one budget are identical.

Session drift: C++ Microban was about 12% slower than on the 0.1.0 day while
C++ XSokoban was about 7% faster, so only within-session ratios are compared.

Binaries: Rust `199cc73862816d891239535caffe602da3201d4e52e513a091e0109914882fab`,
C++ `16f46d56850fd7d16d0075e85a6cd4bfd29551979ad6d916a3fe25081267d2f2`.
The 0.1.0 commit `e530e92` rebuilt today produced 94,231 / 28,709 in 160.6 s
and serves as the same-session pre-change reference.

```text
8cc56b8d1a72b5b8786548dbee24be6043cdba2ead6e9e11f8eb1decd7d3f4a0 *target/bench-011/results.txt
ec1e0c431bd50a7088b4c3bd8aa7140b4abd092f467473cf4808abf95229d47b *target/bench-011/xs-old/times.txt
9a23236af1f085b54d17465a95912aa9bd57021f4476f1e006dcd757e714ed91 *target/bench-011/xs-old/solutions.sok
4ab68f46b94147a45993d871bbbbf9e555bb73a84dacd43733c12bb7b70a698c *target/bench-011/mb-rust-r1/solutions.sok
a567ae6ac1cfa077515a6efa0b80e8088c9780709a3401dcb9bc995ec225a81a *target/bench-011/xs-rust-r2/solutions.sok
```
