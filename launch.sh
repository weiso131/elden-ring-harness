#!/bin/sh
# Usage: [ERH_GROUPS=a,b] [MANGOHUD=0] [ERH_LOG_ROOT=dir] ./launch.sh [--title] [--run COMMAND] [NAME]
#   Settings also come from ./config (see config.example); the environment wins.
#   Recordings go to $ERH_LOG_ROOT/NAME (default ./out/NAME); commands and
#   status go to ./out/NAME, where scripts and erh write. NAME defaults to the
#   date and time.
#   --title presses through the title screen into the last save
#   (scripts/title.py); keep the mouse over the game window.
#   COMMAND, e.g. "scripts/boss.sh radahn-dlc", runs from this directory once
#   the mod answers, with ERH_DIR set to ./out/NAME, so a run needs no command
#   typed elsewhere: presses only reach a focused game.
set -e
here=$(cd "$(dirname "$0")" && pwd)
if [ -f "$here/config" ]; then
	# Only fill in what the environment does not already set.
	env_root=${ERH_LOG_ROOT-} env_groups=${ERH_GROUPS-} env_hud=${MANGOHUD-}
	. "$here/config"
	ERH_LOG_ROOT=${env_root:-${ERH_LOG_ROOT-}}
	ERH_GROUPS=${env_groups:-${ERH_GROUPS-}}
	MANGOHUD=${env_hud:-${MANGOHUD-}}
fi
[ -n "${ERH_GROUPS-}" ] && export ERH_GROUPS
run= title=
while [ $# -gt 0 ]; do
	case $1 in
	--run) run=$2; shift 2 ;;
	--title) title=1; shift ;;
	*) break ;;
	esac
done
name=${1:-$(date +%Y%m%d-%H%M%S)}
root=$(realpath -m "${ERH_LOG_ROOT:-$here/out}")
out=$root/$name
ctl=$here/out/$name
mkdir -p "$out" "$ctl" "$root/.mount"
[ "$out" = "$ctl" ] || ln -sfn "$out" "$ctl/recording"

# The game runs in the Steam Linux Runtime, which only sees the filesystems
# me3 mounts: those holding the profile's files. An empty package under the
# log root makes me3 mount that disk too.
profile=$ctl/erh.me3
sed "s|^path = '../target/|path = '$here/target/|" "$here/profiles/erh.me3" >"$profile"
printf "\n[[packages]]\npath = '%s'\n" "$root/.mount" >>"$profile"

if [ -n "$title" ]; then
	(
		until [ -f "$ctl/status.tsv" ]; do sleep 1; done
		cd "$here" && ERH_DIR="$ctl" scripts/title.py >"$ctl/title.log" 2>&1
	) &
fi
if [ -n "$run" ]; then
	(
		until [ -f "$ctl/status.tsv" ]; do sleep 1; done
		cd "$here" && ERH_DIR="$ctl" sh -c "$run" >"$ctl/run.log" 2>&1
	) &
fi
# me3 looks for its own Windows binaries in ./target/x86_64-pc-windows-msvc/release
# before its install, so it must not start inside this cargo project.
cd /
# MangoHud's frame time graph shows a stall as it happens; MANGOHUD=0 turns it off.
MANGOHUD=${MANGOHUD:-1} ERH_OUT="$out" ERH_CTL="$ctl" exec me3 launch -p "$profile"
