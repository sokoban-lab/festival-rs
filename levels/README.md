# Levels

## Bundled smoke set

`levels/smoke/smoke.sok` contains 12 self-authored levels used by PR CI:

- 1–3: single-box and multi-box pushes;
- 4–6: corridors and horizontal/vertical tunnels;
- 7–9: multi-room layouts;
- 10–12: corner/deadlock traps.

They are released with this repository and contain no third-party content.

## External benchmarks

The large regression sets are not vendored. Place them under `levels/` in this
layout:

```text
levels/Microban.sok
levels/Microban II.sok
levels/XSokoban.sok
levels/SokHard.sok
...
```

Then run, for example:

```sh
cargo run -p festival-bench -- XSokoban
cargo run -p festival-cli -- XSokoban -level 1 --dump
```

Licenses and attribution:

- **Microban I/II** — David W. Skinner. Freeware; keep the author credit.
- **XSokoban** — original XSokoban distribution; confirm redistribution terms
  before committing.
- **SokHard**, **Sasquatch I–VII**, **Grigr2001/2002**, **Sven**, **Yoshio
  Murase**, **Aymeric** — check each collection's terms and record the source
  and checksum in `reference/LEVEL_SOURCES.md` before use.

Do not add a level set to git until its license allows redistribution.
