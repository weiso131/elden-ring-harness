#!/bin/sh
# Brings a defeated boss back and puts it in a phase, for a measured run.
# Usage: scripts/boss.sh BLOCK GRACE HP_PCT FLAG...
#   BLOCK   map block the boss is in, as `erh status` prints it
#   GRACE   grace to warp to (BonfireWarpParam row id)
#   HP_PCT  HP to set the boss to, e.g. 50 to start a second phase
#   FLAG    flags to clear, e.g. the boss's x800 and x801
# Example (DLC Radahn, second phase): scripts/boss.sh m20_01_00_00 200100 50 20010800 20010801
set -e
erh=$(dirname "$0")/../cli/target/release/erh
block=$1 grace=$2 hp=$3
shift 3

field() { "$erh" status | awk -F'\t' -v k="$1" '$1 == k { print $2 }'; }
wait_for() { # wait_for KEY OP VALUE, OP is = or !=
	for _ in $(seq 120); do
		[ "$(field "$1")" "$2" "$3" ] && return
		sleep 0.5
	done
	echo "timed out waiting for $1 $2 $3" >&2
	exit 1
}

"$erh" nodead on
"$erh" nohit on
for f in "$@"; do "$erh" flag "$f" off; done
# Leave the boss's map so it is unloaded; warping within it keeps old objects.
# A warp starts loading a moment after it is asked for.
"$erh" warp 111000
sleep 3
wait_for block != "$block"
wait_for stage = in_game
"$erh" warp "$grace"
sleep 3
wait_for block = "$block"
# Characters load a moment after the map does.
sleep 5
"$erh" targethp "$hp"
"$erh" status
