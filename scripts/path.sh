#!/bin/sh
# Records a route walked by hand as world positions, and walks it again.
usage() {
	cat <<'USAGE'
Usage: scripts/path.sh record FILE [STEP]
       scripts/path.sh play FILE [RADIUS]

record  appends the player's world position "x y z" to FILE each time it has
        moved STEP meters (default 2) along the ground, until Ctrl-C
play    walks to each point of FILE in turn with walkto, a point counting as
        reached within RADIUS meters (default 1)

Playing pushes the movement stick, so the game window needs focus.
USAGE
}

set -e
here=$(dirname "$0")
erh=$here/../cli/target/release/erh
field() { "$erh" status | awk -F'\t' -v k="$1" '$1 == k { print $2 }'; }

case $1 in -h | --help) usage; exit 0 ;; esac
[ $# -ge 2 ] || { usage >&2; exit 2; }
file=$2
case $1 in
record)
	step=${3:-2}
	last=
	while :; do
		pos=$(field world)
		if [ -n "$pos" ] && echo "$last $pos" | awk -v s="$step" \
			'NF == 3 || ($1 - $4) ^ 2 + ($3 - $6) ^ 2 >= s * s { ok = 1 } END { exit !ok }'; then
			echo "$pos" | tee -a "$file"
			last=$pos
		fi
		sleep 0.2
	done
	;;
play)
	radius=${3:-1}
	while read -r x y z; do
		"$erh" walkto "$x" "$z" "$radius" >/dev/null
		sleep 0.2
		while :; do
			case $(field walk) in
			done*) break ;;
			timeout* | stopped) echo "stuck before $x $y $z" >&2; exit 1 ;;
			esac
			sleep 0.1
		done
	done <"$file"
	;;
*) usage >&2; exit 2 ;;
esac
