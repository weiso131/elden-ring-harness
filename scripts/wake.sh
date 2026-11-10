#!/bin/sh
# Wakes a boss left idle by skipping its fog gate: while locked on, walks at
# it and attacks until its HP drops, then stops.
# Usage: scripts/wake.sh [SECONDS]   (default 30; needs the lock-on already)
# Presses only reach the game while its window has focus.
set -e
erh=$(dirname "$0")/../cli/target/release/erh
field() { "$erh" status | awk -F'\t' -v k="$1" '$1 == k { print $2 }'; }
[ "$(field locked)" = on ] || { echo "not locked on" >&2; exit 1; }
start=$(field target_hp)
end=$(($(date +%s) + ${1:-30}))
while [ "$(date +%s)" -lt "$end" ]; do
	hp=$(field target_hp)
	[ -n "$hp" ] && [ "$hp" -lt "$start" ] && { echo "hit: hp $start -> $hp"; exit 0; }
	dist=$(field target_dist)
	# Walking with the lock on goes straight at the target.
	if [ "${dist%.*}" -gt 3 ] 2>/dev/null; then
		"$erh" press forward 15 >/dev/null
	else
		"$erh" press attack 3 >/dev/null
	fi
	sleep 0.3
done
echo "boss never took a hit" >&2
exit 1
