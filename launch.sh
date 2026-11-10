#!/bin/sh
# Usage: [ERH_GROUPS=a,b] [MANGOHUD=0] ./launch.sh [--run COMMAND] [OUT_DIR]
#   OUT_DIR defaults to ./out/<date-time>.
#   COMMAND, e.g. "scripts/boss.sh radahn-dlc", runs from this directory once
#   the mod answers, with ERH_DIR set to OUT_DIR, so a run needs no command
#   typed elsewhere: presses only reach a focused game.
set -e
here=$(cd "$(dirname "$0")" && pwd)
run=
if [ "$1" = --run ]; then
	run=$2
	shift 2
fi
out=$(realpath -m "${1:-out/$(date +%Y%m%d-%H%M%S)}")
mkdir -p "$out"
if [ -n "$run" ]; then
	(
		until [ -f "$out/status.tsv" ]; do sleep 1; done
		cd "$here" && ERH_DIR="$out" sh -c "$run" >"$out/run.log" 2>&1
	) &
fi
# me3 looks for its own Windows binaries in ./target/x86_64-pc-windows-msvc/release
# before its install, so it must not start inside this cargo project.
cd /
# MangoHud's frame time graph shows a stall as it happens; MANGOHUD=0 turns it off.
MANGOHUD=${MANGOHUD:-1} ERH_OUT="$out" exec me3 launch -p "$here/profiles/erh.me3"
