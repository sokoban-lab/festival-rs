#!/usr/bin/env python3
"""Compare two packing plans produced by `refcheck plan` and `festival-bench packing-plan`.

Plans are lists of (from, to, until) cell indices.  The plan text is identical
when the port reproduces the C++ search exactly.  When it differs, call the
plans equivalent if both are internally consistent and park the same initial
boxes on the same final targets; intermediate waypoints may differ.

Exit status: 0 for identical or equivalent, 1 for a semantic difference,
2 for malformed input.
"""

import sys


def read_plan(path):
    steps = []
    declared = None
    with open(path, "r", encoding="utf-8", errors="replace") as handle:
        for raw in handle:
            line = raw.strip()
            if line.startswith("plan "):
                declared = int(line.split()[1])
                continue
            parts = line.split()
            if len(parts) != 3:
                continue
            try:
                steps.append(tuple(int(part) for part in parts))
            except ValueError:
                continue
    if declared is None:
        raise ValueError(f"{path}: missing plan header")
    if declared != len(steps):
        raise ValueError(f"{path}: header says {declared}, found {len(steps)}")
    return steps


def validate(steps):
    """Check chain consistency: every `from` is initial/unknown/earlier `to`."""
    seen = set()
    for index, (source, target, until) in enumerate(steps):
        if source >= 0 and source not in seen:
            return f"step {index} pulls from {source}, which is not an earlier target"
        if until != -1:
            if not (index < until < len(steps)):
                return f"step {index} has out-of-range until {until}"
            if steps[until][0] != target:
                return f"step {index} until {until} does not pull from {target}"
        seen.add(target)
    return None


def signature(steps):
    initial = sorted(source for source, _target, _until in steps if source == -2)
    final = sorted(
        target for _source, target, until in steps if until == -1
    )
    return (tuple(initial), tuple(final))


def main(argv):
    if len(argv) != 3:
        print("usage: compare_packing_plans.py <cpp-plan> <rust-plan>", file=sys.stderr)
        return 2
    try:
        cpp = read_plan(argv[1])
        rust = read_plan(argv[2])
    except (OSError, ValueError) as err:
        print(f"error: {err}", file=sys.stderr)
        return 2

    for name, plan in (("cpp", cpp), ("rust", rust)):
        problem = validate(plan)
        if problem:
            print(f"different: invalid {name} plan: {problem}")
            return 1

    if cpp == rust:
        print("exact")
        return 0

    cpp_sig = signature(cpp)
    rust_sig = signature(rust)
    if cpp_sig == rust_sig:
        print(
            "equivalent: same initial boxes and final targets "
            f"({len(cpp)} vs {len(rust)} steps)"
        )
        return 0

    print("different: final parking sets or initial boxes differ")
    print(f"  cpp  plan {cpp}")
    print(f"  rust plan {rust}")
    return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
