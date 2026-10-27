# C++ baseline

Status: baseline binary not yet built on this machine.

## Source

- Festival 3.1, commit `f7c023122f4e93e5e649ec856a9721b592d1108b`
  (`2025-10-04`, `feat: initial commit`)
- Local source directory used for the port: `D:\Users\sms\Desktop\Repos\festival`
- Author: Yaron Shoham (copyright 2018-2022)

## Toolchain

| Tool | Found | Notes |
|---|---|---|
| Rust | 1.99.1 | used for this port |
| clang/clang++ | 23.1.4 (x86_64-pc-windows-msvc) | present, but the C++ tree expects POSIX threads and MinGW/`conio.h` |
| gcc/g++ | not installed | Phase 0 blocker |

The planned baseline builds:

```sh
# Windows / MSYS2 UCRT64 (from the Festival README)
gcc -O3 *.cpp -pthread -DTHREADS -o festival.exe

# Linux (verify the exact include fixes first; sol.cpp uses <malloc.h>)
g++ -O3 -DLINUX -DTHREADS *.cpp -pthread -o festival-cpp
```

## Reproducible commands

Once a compiler is available, record here:

1. compiler version and full command line;
2. smoke-set invocation, e.g. `festival.exe levels/smoke/smoke.sok -level 1 -time 60 -cores 1`;
3. `solutions.sok` / `times.txt` checksums;
4. `refcheck` output checksums for hash/static/deadlock/trace modes.

## Next actions

1. Install MSYS2 UCRT64 (`mingw-w64-ucrt-x86_64-gcc`) or WSL2 Ubuntu + g++.
2. Build the C++ binary and a `reference/refcheck.cpp` driver:
   `hash`, `static`, `deadlock`, `trace`, `solve`.
3. Capture smoke-set outputs as golden fixtures.
4. Compare Rust static analysis against `refcheck static` per level.
