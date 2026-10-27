#!/usr/bin/env bash
# Fetch the Microban I corpus and verify its checksum.
set -euo pipefail

cd "$(dirname "$0")/.."

URL="https://raw.githubusercontent.com/thisisandreeeee/sokoban-solver/main/sokoban/levels/microban.xsb"
DEST="levels/corpus/microban.xsb"

mkdir -p "$(dirname "$DEST")"
curl -fsSL "$URL" -o "$DEST"
sha256sum -c levels/corpus/microban.xsb.sha256
