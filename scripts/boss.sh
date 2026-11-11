#!/bin/sh
# Brings a defeated boss back, optionally in a later phase, for a run.
usage() {
	cat <<'USAGE'
Usage: scripts/boss.sh [BOSS] [options]

Without BOSS, asks for everything. BOSS is an id from bosses.tsv; --list shows them.

Options:
  --list          list the bosses in bosses.tsv and exit
  --nodead        keep the player alive, HP stops at 1 (default)
  --no-nodead     let the player die
  --nohit         enemy attacks do not hit the player (grabs that kill
                  through nodead included)
  --hp PCT        set the boss's HP to PCT% once it is there; defaults to the
                  table's phase HP
  --no-hp         leave the boss's HP alone, for a phase-one fight
  --no-lock       do not turn to the boss and lock on at the end
  -h, --help      show this help

Presses (the lock-on) only reach the game while its window has focus: keep
the mouse over it.
USAGE
}

set -e
here=$(dirname "$0")
erh=$here/../cli/target/release/erh
table=$here/../bosses.tsv

rows() { grep -v '^#' "$table" | grep -v '^$'; }
list() { rows | awk -F'\t' '{ printf "%-12s %s%s\n", $1, $11, ($10 == "yes" ? "" : "  (not checked in game)") }'; }

boss= nodead=on nohit=off hp=table lockon=yes
while [ $# -gt 0 ]; do
	case $1 in
	-h | --help) usage; exit 0 ;;
	--list) list; exit 0 ;;
	--nodead) nodead=on ;;
	--no-nodead) nodead=off ;;
	--nohit) nohit=on ;;
	--hp) hp=$2; shift ;;
	--no-hp) hp=none ;;
	--no-lock) lockon=no ;;
	-*) echo "unknown option: $1" >&2; usage >&2; exit 2 ;;
	*) boss=$1 ;;
	esac
	shift
done

ask() { # ask QUESTION DEFAULT -> answer on stdout
	printf '%s [%s] ' "$1" "$2" >/dev/tty
	read -r a </dev/tty
	echo "${a:-$2}"
}
if [ -z "$boss" ]; then
	[ -t 0 ] || { echo "no boss given and no terminal to ask on" >&2; exit 2; }
	list | nl -w2 -s') ' >/dev/tty
	n=$(ask "boss (number or id)" 1)
	case $n in
	*[!0-9]*) boss=$n ;;
	*) boss=$(rows | sed -n "${n}p" | cut -f1) ;;
	esac
	case $(ask "keep the player alive (nodead)? y/n" y) in n*) nodead=off ;; esac
	case $(ask "enemy attacks miss the player (nohit)? y/n" n) in y*) nohit=on ;; esac
	phase=$(rows | awk -F'\t' -v b="$boss" '$1 == b { print $7 }')
	if [ "$phase" != - ] && [ -n "$phase" ]; then
		hp=$(ask "set the boss's HP to % (none to skip)" "$phase")
		[ "$hp" = none ] || [ -n "$hp" ] || hp=none
	else
		hp=$(ask "set the boss's HP to % (none to skip)" none)
	fi
fi

row=$(rows | awk -F'\t' -v b="$boss" '$1 == b')
[ -n "$row" ] || { echo "no boss '$boss' in bosses.tsv; --list shows them" >&2; exit 2; }
col() { echo "$row" | cut -f"$1"; }
block=$(col 2) grace=$(col 3) flags=$(col 4) fight=$(col 5) fog=$(col 6) lock=$(col 8) esc=$(col 9)
[ "$hp" = table ] && hp=$(col 7)
[ "$hp" = - ] && hp=none

locked=no
wait_lock() {
	for _ in $(seq 60); do
		case $(field lockon) in
		done*) return ;;
		"no lock" | "no target") echo "not locked on; is the game window focused?" >&2; exit 1 ;;
		esac
		sleep 0.1
	done
	echo "timed out locking on" >&2
	exit 1
}

field() { "$erh" status | awk -F'\t' -v k="$1" '$1 == k { print $2 }'; }
wait_for() { # wait_for KEY OP VALUE [SECONDS], OP is = != or -gt
	for _ in $(seq $((${4:-60} * 2))); do
		[ "$(field "$1")" "$2" "$3" ] 2>/dev/null && return
		sleep 0.5
	done
	echo "timed out waiting for $1 $2 $3" >&2
	exit 1
}

# Started by launch.sh --run, the game may still be on the title screen.
wait_for stage = in_game 600
"$erh" nodead "$nodead"
"$erh" nohit "$nohit"
for f in $(echo "$flags" | tr , ' '); do "$erh" flag "$f" off; done
# Leave the boss's map so it is unloaded: warping within it keeps objects left
# from the last fight. A warp starts loading a moment after it is asked for.
"$erh" warp 111000
sleep 3
wait_for block != "$block"
wait_for stage = in_game
"$erh" warp "$grace"
sleep 3
wait_for block = "$block"
# Characters load a moment after the map does.
wait_for chars -gt 0 30
sleep 1
# Warping into the arena skips the fog gate, whose event starts the fight;
# these are the flags that event sets, found by diffing flagdumps around it.
[ "$fight" = - ] || for f in $(echo "$fight" | tr , ' '); do "$erh" flag "$f" on; done
if [ "$fog" != - ]; then
	# The fight, AI included, starts from the fog gate's own event, so walk
	# through it as a player would: stand just outside and press action.
	"$erh" tp $fog
	sleep 1
	"$erh" press action 3
	# Walking through the fog takes a few seconds of animation.
	sleep 6
fi

if [ "$hp" != none ]; then
	if [ "$lock" = yes ]; then
		# The phase only changes while locked on, and a second lockon press
		# drops the lock, so the mod presses once and sets HP as it locks.
		"$erh" lockhp "$hp"
		wait_lock
		locked=yes
	else
		# PCT@RANK picks a character by HP rank, for fights where the one to
		# hit is not the nearest of the big ones.
		case $hp in
		*@*) "$erh" targethp "${hp%@*}" "${hp#*@}" ;;
		*) "$erh" targethp "$hp" ;;
		esac
	fi
	if [ "$esc" = yes ]; then
		# The phase change cutscene starts a moment after the HP drops. One
		# Esc skips it; a second opens the menu, which the mod's presses
		# cannot close. 235 is Esc in the virtual input, found with scan.
		sleep 2
		"$erh" rawpress 235 3
		# The cutscene drops the lock.
		sleep 2
		locked=no
	fi
fi
# The mod turns the player to the boss and presses lockon, retrying since a
# press with the boss out of view only swings the camera behind the player.
if [ "$lockon" = yes ] && [ "$locked" = no ]; then
	"$erh" lockon
	wait_lock
fi
"$erh" status
