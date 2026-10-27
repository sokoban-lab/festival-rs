#!/usr/bin/env bash
# Build reference/refcheck.cpp against the upstream Festival 3.1 sources.
# The upstream main() is renamed so refcheck can own the entry point.
set -euo pipefail

cd "$(dirname "$0")/.."

UPSTREAM="${FESTIVAL_CPP_DIR:-../festival}"
CXX="${CXX:-clang++}"
OUT="${OUT:-target/refcheck.exe}"
OBJ="${OBJ:-target/refcheck-obj}"

if [ ! -d "$UPSTREAM" ]; then
    echo "Festival C++ sources not found at $UPSTREAM" >&2
    echo "Set FESTIVAL_CPP_DIR to the upstream checkout." >&2
    exit 1
fi

mkdir -p "$(dirname "$OUT")" "$OBJ"

for src in "$UPSTREAM"/*.cpp; do
    name="$(basename "$src" .cpp)"
    "$CXX" -O3 -DTHREADS -Dmain=festival_main -I reference/cpp-shim \
        -c "$src" -o "$OBJ/$name.o"
done

"$CXX" -O3 -DTHREADS -I reference/cpp-shim -I "$UPSTREAM" \
    -c reference/refcheck.cpp -o "$OBJ/refcheck.o"

# `plan` calls packing_search outside the solver's usual entry path; the
# default 1 MB main-thread stack is not enough for the advisor scratch arrays
# at -O3, so give refcheck extra headroom. This does not change solver logic.
"$CXX" "$OBJ"/*.o -Wl,/STACK:8388608 -o "$OUT"
echo "built $OUT"
