#!/usr/bin/env bash
# Export the link environment cargo-fuzz needs on this host.
#
# The Visual Studio installation here has no x64 AddressSanitizer component,
# so `link.exe /INFERASANLIBS` cannot find VCASAN.lib / stl_asan.lib. LLVM ships
# the x86_64 ASan runtime, so this script points LIB at LLVM's compiler-rt and
# adds marker-only stubs for the two MSVC libraries that Rust's ASan link line
# names but does not otherwise use.
#
# Source it, do not execute it:
#   . scripts/fuzz_env.sh
set -euo pipefail

RESOURCE_DIR="$(clang++ -print-resource-dir 2>/dev/null || true)"
if [ -z "$RESOURCE_DIR" ]; then
    echo "fuzz_env: clang++ -print-resource-dir failed" >&2
    return 1 2>/dev/null || exit 1
fi
WIN_LIB="$(cygpath -u "$RESOURCE_DIR")/lib/windows"
if [ ! -f "$WIN_LIB/clang_rt.asan_dynamic-x86_64.lib" ]; then
    echo "fuzz_env: no x86_64 ASan runtime in $WIN_LIB" >&2
    return 1 2>/dev/null || exit 1
fi

STUB_DIR=target/fuzz-libs
mkdir -p "$STUB_DIR"
if [ ! -f "$STUB_DIR/stl_asan.lib" ] || [ ! -f "$STUB_DIR/VCASAN.lib" ]; then
    CLANG_CL="$(command -v clang-cl || true)"
    LLVM_LIB="$(command -v llvm-lib || true)"
    if [ -z "$CLANG_CL" ] || [ -z "$LLVM_LIB" ]; then
        echo "fuzz_env: clang-cl and llvm-lib are required for the ASan stubs" >&2
        return 1 2>/dev/null || exit 1
    fi
    printf 'int festival_asan_stub;\n' > "$STUB_DIR/stub_stl.c"
    printf 'int __you_must_link_with_VCAsan_lib;\n' > "$STUB_DIR/stub_vcasan.c"
    for pair in "stub_stl:stl_asan" "stub_vcasan:VCASAN"; do
        src="${pair%%:*}"
        lib="${pair##*:}"
        MSYS_NO_PATHCONV=1 MSYS2_ARG_CONV_EXCL='*' "$CLANG_CL" /c \
            "$STUB_DIR/$src.c" "/Fo:$STUB_DIR/$src.obj"
        MSYS_NO_PATHCONV=1 MSYS2_ARG_CONV_EXCL='*' "$LLVM_LIB" \
            "/out:$STUB_DIR/$lib.lib" "$STUB_DIR/$src.obj"
    done
fi

export LIB="$(cygpath -w -a "$STUB_DIR");$(cygpath -w "$WIN_LIB");${LIB:-}"
export PATH="$(dirname "$(command -v clang++)"):$WIN_LIB:$PATH"
