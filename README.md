# festival-rs

An AI-assisted Rust port of Festival 3.1, the Sokoban solver by Yaron Shoham.

## Benchmarks

Compared with Festival 3.1 on the same host; both solvers solve every level in these sets.

| Level set        | Metric     | festival-rs |       C++ | Rust / C++ |
| ---------------- | ---------- | ----------: | --------: | ---------: |
| Microban I (155) | Total time |     8.348 s |  18.852 s |  **0.443** |
|                  | Peak RSS   |    270.7 MB |  735.4 MB |  **0.368** |
| XSokoban (90)    | Total time |   102.057 s | 181.424 s |  **0.563** |
|                  | Peak RSS   |    322.8 MB |  855.2 MB |  **0.377** |

Times and RSS cover the full level-set batch; see [`docs/benchmarks.md`](docs/benchmarks.md) for details. Rust-only optimizations that go beyond the C++ reference are tracked in [`docs/optimization.md`](docs/optimization.md).

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
