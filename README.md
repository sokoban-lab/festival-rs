# festival-rs

An AI-assisted Rust port of Festival 3.1, the Sokoban solver by Yaron Shoham.

## Benchmarks

Compared with Festival 3.1 on the same host; both solvers solve every level in these sets.

| Level set        | Metric     | festival-rs |       C++ | Rust / C++ |
| ---------------- | ---------- | ----------: | --------: | ---------: |
| Microban I (155) | Total time |     7.602 s |  21.077 s |  **0.361** |
|                  | Peak RSS   |    276.7 MB |  735.5 MB |  **0.376** |
| XSokoban (90)    | Total time |   107.848 s | 192.793 s |  **0.559** |
|                  | Peak RSS   |    301.1 MB |  864.7 MB |  **0.348** |

Times and RSS cover the full level-set batch; absolute times drift with host load, so compare the ratios. See [`docs/benchmarks.md`](docs/benchmarks.md) for details. Rust-only optimizations that go beyond the C++ reference are tracked in [`docs/optimization.md`](docs/optimization.md).

## Known Differentials

- `--alg` is honored for every core count; C++ 3.1 ignores it when `--cores > 1`.
- The CLI requires an explicit level set, has no default `XSokoban`, and rejects legacy single-dash options and `--from`/`--to` with a replacement hint.
- Strategy H (Dragonfly + NAIVE) matches the C++ solve set with a 0.3% move and push quality difference; strategies A and G take different solutions on a few levels while solving the same set.
- Core selection uses 1/2/4/8 rounded down from the host CPU count; C++'s RAM-based thresholds are not applied.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT), at your option.

festival-rs is a Rust port of Festival 3.1, copyright 2018–2022 Yaron Shoham.
The
original source asks users to credit the program and its author; that credit is retained in this README. Level sets keep their own licenses; see [`levels/README.md`](levels/README.md).
