# festival-rs

An AI-assisted Rust port of [Festival](https://festival-solver.site/).

## Usage

```sh
# Solve every level with the default 600 s per-level budget.
festival-rs <path/to/xsb>

# Solve levels 1-20 with a 60 s budget.
festival-rs <path/to/xsb> --level 1..20 --timeout 60

# Lower a single worker's memory budget.
festival-rs <path/to/xsb> --threads 1 --memory 512M

# Print static analysis with the board and room dump.
festival-rs <path/to/xsb> --analyse --dump

# Machine-readable output.
festival-rs <path/to/xsb> --format json
```

## Benchmarks

Compared with Festival 3.1 on the same host; both solvers solve every level in these sets.

| Level set        | Metric     | festival-rs |       C++ | Rust / C++ |
| ---------------- | ---------- | ----------: | --------: | ---------: |
| Microban I (155) | Total time |     4.669 s |  24.569 s |  **0.190** |
|                  | Peak RSS   |    501.5 MB |  736.1 MB |  **0.681** |
| XSokoban (90)    | Total time |   113.975 s | 223.050 s |  **0.511** |
|                  | Peak RSS   |    369.9 MB |  851.3 MB |  **0.435** |

Times and RSS cover the full level-set batch; absolute times drift with host load, so compare the ratios. See [`docs/benchmarks.md`](docs/benchmarks.md) for details. Rust-only optimizations that go beyond the C++ reference are tracked in [`docs/optimization.md`](docs/optimization.md).

## Known Differentials

- Algorithm H (Dragonfly + NAIVE) matches the C++ solve set with a 0.3% move and push quality difference; algorithms A and G take different solutions on a few levels while solving the same set.
- Thread selection uses 1/2/4/8 rounded down from the host CPU count; C++'s RAM-based thresholds are not applied.
- The CLI interface is completely different.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT), at your option.

The original Festival was developed by Yaron Shoham, copyright 2018–2022. Level sets keep their own licenses; see [`levels/README.md`](levels/README.md).
