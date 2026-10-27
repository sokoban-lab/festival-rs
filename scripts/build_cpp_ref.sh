#!/usr/bin/env bash
# Build the upstream Festival 3.1 C++ binary used for differential runs.
set -euo pipefail

cd "$(dirname "$0")/.."

UPSTREAM="${FESTIVAL_CPP_DIR:-../festival}"
CXX="${CXX:-clang++}"
OUT="${OUT:-target/festival-cpp.exe}"

if [ ! -d "$UPSTREAM" ]; then
    echo "Festival C++ sources not found at $UPSTREAM" >&2
    echo "Set FESTIVAL_CPP_DIR to the upstream checkout." >&2
    exit 1
fi

mkdir -p "$(dirname "$OUT")"

# reference/cpp-shim provides a Windows pthread shim; on platforms with a real
# pthread.h it is harmless but unnecessary.
"$CXX" -O3 -DTHREADS -I reference/cpp-shim "$UPSTREAM"/*.cpp -o "$OUT"
echo "built $OUT"
