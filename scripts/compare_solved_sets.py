#!/usr/bin/env python3
"""Compare per-level solve sets in two Festival `times.txt` files.

The C++ and Rust writers share the same row layout. Exit status is 0 when the
number of differing levels is within `--tolerance` (a fraction of the shared
level count), 1 otherwise.
"""

import argparse
import re
import sys

ROW = re.compile(r"^\s*(\d+)\s+\d{2}:\d{2}:\d{2}\s+Festival 3\.1\s+(.*?)\s{2,}")


def parse(path):
    levels = {}
    with open(path, encoding="utf-8", errors="replace") as handle:
        for line in handle:
            match = ROW.match(line)
            if not match:
                continue
            result = match.group(2).strip()
            levels[int(match.group(1))] = result
    return levels


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("cpp_times")
    parser.add_argument("rust_times")
    parser.add_argument("--tolerance", type=float, default=0.0)
    parser.add_argument("--log", default=None)
    args = parser.parse_args()

    cpp = parse(args.cpp_times)
    rust = parse(args.rust_times)
    shared = sorted(set(cpp) & set(rust))
    cpp = {level: cpp[level] for level in shared}
    rust = {level: rust[level] for level in shared}

    diffs = [
        (level, cpp[level], rust[level])
        for level in shared
        if cpp[level] != rust[level]
    ]
    lines = [
        f"levels {len(shared)}; cpp solved {sum(v == 'OK' for v in cpp.values())}; "
        f"rust solved {sum(v == 'OK' for v in rust.values())}; diffs {len(diffs)}"
    ]
    for level, cpp_result, rust_result in diffs:
        lines.append(f"  level {level}: cpp {cpp_result!r} vs rust {rust_result!r}")
    text = "\n".join(lines)
    print(text)
    if args.log:
        with open(args.log, "a", encoding="utf-8") as handle:
            handle.write(text + "\n")

    allowed = args.tolerance * max(len(shared), 1)
    return 0 if len(diffs) <= allowed + 1e-9 else 1


if __name__ == "__main__":
    sys.exit(main())
