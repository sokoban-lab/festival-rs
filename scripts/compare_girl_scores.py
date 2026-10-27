#!/usr/bin/env python3
"""Compare GIRL push/pull score traces (C++ refcheck vs festival-bench).

Each line is:
  hash push_packed push_tgt push_connectivity push_dist push_overlap push_value
       pull_tgt pull_connectivity pull_dist pull_value

Integer fields must match exactly; values must agree within 1e-5.
"""

import sys


def parse(path):
    rows = []
    with open(path, "r", encoding="ascii") as fp:
        for line in fp:
            fields = line.split()
            if len(fields) != 11:
                raise SystemExit(f"{path}: malformed line: {line!r}")
            rows.append(
                (
                    int(fields[0]),
                    *(int(value) for value in fields[1:6]),
                    float(fields[6]),
                    *(int(value) for value in fields[7:10]),
                    float(fields[10]),
                )
            )
    return rows


def main():
    if len(sys.argv) != 3:
        raise SystemExit("usage: compare_girl_scores.py <cpp.txt> <rust.txt>")

    cpp = parse(sys.argv[1])
    rust = parse(sys.argv[2])

    if len(cpp) != len(rust):
        raise SystemExit(f"row count differs: cpp {len(cpp)} rust {len(rust)}")

    mismatches = 0
    max_float_diff = 0.0
    for index, (a, b) in enumerate(zip(cpp, rust)):
        if a[0] != b[0]:
            print(f"row {index}: hash {a[0]} != {b[0]}")
            mismatches += 1
            continue
        if a[1:6] != b[1:6] or a[7:10] != b[7:10]:
            print(f"row {index} (hash {a[0]}): fields {a[1:6]}/{a[7:10]} != {b[1:6]}/{b[7:10]}")
            mismatches += 1
            continue
        for label, av, bv in (("push", a[6], b[6]), ("pull", a[10], b[10])):
            diff = abs(av - bv)
            max_float_diff = max(max_float_diff, diff)
            if diff >= 1e-5:
                print(f"row {index} (hash {a[0]}): {label} value {av!r} != {bv!r}")
                mismatches += 1

    if mismatches:
        print(f"FAIL: {mismatches} mismatches over {len(cpp)} rows")
        return 1
    print(f"ok: {len(cpp)} rows, max |value diff| {max_float_diff:.3e}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
