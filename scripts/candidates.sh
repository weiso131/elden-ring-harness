#!/bin/sh
# Lists what a bosses.tsv row for a dungeon map could use: the map's graces and
# its x8xx flags that are on. Needs `erh graces` and `erh flagdump all` taken in
# RUN_DIR on a character that has beaten the boss.
# Usage: scripts/candidates.sh BLOCK [RUN_DIR], e.g. scripts/candidates.sh m20_01_00_00
# The boss flags seen so far follow AABB08xx for map mAA_BB, in pairs like
# 20010800/20010801 for m20_01; Placidusax's did not show up this way, and
# overworld bosses (m60, m61) are numbered differently and not covered.
set -e
block=$1
dir=${2:-$(ls -d "$(dirname "$0")"/../out/*/ | tail -1)}
aa=$(echo "$block" | cut -c2-3) bb=$(echo "$block" | cut -c5-6)
echo "graces in $block (row, entity, unlocked):"
awk -F'\t' -v a="$((10#$aa))" -v b="$((10#$bb))" \
	'NR > 1 && $3 == a && $4 == b { print "  " $1, $2, $7 }' "$dir/graces.tsv"
echo "flags on in ${aa}${bb}0800-${aa}${bb}0899:"
grep -E "^${aa}${bb}08[0-9][0-9]\$" "$dir/flags-all.txt" | sed 's/^/  /'
