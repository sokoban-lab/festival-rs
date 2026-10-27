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

## Still missing

`refcheck hash|static|deadlock|trace` has not been implemented yet. The current
differential covers end-to-end solve results and independent solution replay;
per-node trace comparison remains a Phase 3/6 task.
