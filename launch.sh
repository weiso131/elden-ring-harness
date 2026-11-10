#!/bin/sh
# Usage: ./launch.sh [OUT_DIR]   (default: ./out)
set -e
here=$(cd "$(dirname "$0")" && pwd)
out=$(realpath -m "${1:-out}")
mkdir -p "$out"
# me3 looks for its own Windows binaries in ./target/x86_64-pc-windows-msvc/release
# before its install, so it must not start inside this cargo project.
cd /
ERH_OUT="$out" exec me3 launch -p "$here/profiles/erh.me3"
