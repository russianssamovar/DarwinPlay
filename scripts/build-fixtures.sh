#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BUILD="$ROOT/fixtures/d3d11-triangle/build"
COMPILER="${CXX:-x86_64-w64-mingw32-g++}"

if ! command -v "$COMPILER" >/dev/null 2>&1; then
    echo "Missing $COMPILER. Install a MinGW-w64 x86_64 C++ compiler." >&2
    exit 1
fi

cmake -S "$ROOT/fixtures/d3d11-triangle" -B "$BUILD" \
    -DCMAKE_SYSTEM_NAME=Windows \
    -DCMAKE_CXX_COMPILER="$COMPILER" \
    -DCMAKE_BUILD_TYPE=Release
cmake --build "$BUILD" --config Release
find "$BUILD" -name 'darwinplay-d3d11-fixture.exe' -print -quit
