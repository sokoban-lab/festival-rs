# festival-rs

An AI-assisted Rust port of [Festival](https://festival-solver.site/).

## Usage

```sh
# Solve every level with the default 600 s per-level budget.
festival-rs <path/to/xsb>

# Solve levels 1-20 with a 60 s budget.
festival-rs <path/to/xsb> --level 1..20 --timeout 60

# Give a single worker a larger memory budget.
festival-rs <path/to/xsb> --threads 1 --memory 2G

# Print static analysis with the board and room dump.
festival-rs <path/to/xsb> --analyse --dump

# Machine-readable output.
festival-rs <path/to/xsb> --format json
```

## Benchmarks

Compared with Festival 3.1 on the same host; both solvers solve every level in these sets.

| Level set        | Metric     | festival-rs |       C++ | Rust / C++ |
| ---------------- | ---------- | ----------: | --------: | ---------: |
| Microban I (155) | Total time |     4.646 s |  18.968 s |  **0.245** |
|                  | Peak RSS   |    242.3 MB |  771.7 MB |  **0.314** |
| XSokoban (90)    | Total time |   102.914 s | 190.683 s |  **0.540** |
|                  | Peak RSS   |    339.7 MB |  932.8 MB |  **0.364** |

Times and RSS cover the full level-set batch; absolute times drift with host load, so compare the ratios. See [`docs/benchmarks.md`](docs/benchmarks.md) for details. Rust-only optimizations that go beyond the C++ reference are tracked in [`docs/optimization.md`](docs/optimization.md).

## Known Differentials

- `--algorithm` is honored for every thread count; C++ 3.1 ignores `-alg` when `-cores > 1`.
- The CLI requires an explicit level set, has no default `XSokoban`, and rejects legacy single-dash options, renamed 0.1.x options (`--time`, `--alg`, `--cores`, `--out-dir`), and `--from`/`--to` with a replacement hint.
- Algorithm H (Dragonfly + NAIVE) matches the C++ solve set with a 0.3% move and push quality difference; algorithms A and G take different solutions on a few levels while solving the same set.
- Thread selection uses 1/2/4/8 rounded down from the host CPU count; C++'s RAM-based thresholds are not applied.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT), at your option.

The original Festival was developed by Yaron Shoham, copyright 2018–2022. Level sets keep their own licenses; see [`levels/README.md`](levels/README.md).
