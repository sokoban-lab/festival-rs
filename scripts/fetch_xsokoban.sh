#!/usr/bin/env bash
# Fetch XSokoban 3.3c screens and assemble them into a `.sok` level set.
#
# XSokoban is distributed in the public domain and may be freely
# redistributed (see the upstream README). The screen files are plain XSB.
set -euo pipefail

cd "$(dirname "$0")/.."

BASE="https://raw.githubusercontent.com/andrewcmyers/xsokoban/master/screens"
DEST="levels/corpus/xsokoban.sok"
TMP="target/xsokoban-screens"

mkdir -p "$TMP" "$(dirname "$DEST")"
for i in $(seq 1 90); do
    curl -fsSL "$BASE/screen.$i" -o "$TMP/screen.$i"
done

python=python3
if ! command -v "$python" >/dev/null 2>&1; then
    python=python
fi

"$python" - "$TMP" "$DEST" <<'PY'
import sys
src, dest = sys.argv[1], sys.argv[2]
out = []
for i in range(1, 91):
    with open(f"{src}/screen.{i}", "rb") as handle:
        text = handle.read().decode("latin1").replace("\r\n", "\n").rstrip("\n")
    lines = text.split("\n")
    while lines and not lines[-1].strip():
        lines.pop()
    out.append("\n".join(lines))
    out.append(f"Title: XSokoban {i}")
with open(dest, "w", newline="\n") as handle:
    handle.write("\n".join(out) + "\n")
PY

sha256sum -c levels/corpus/xsokoban.sok.sha256
