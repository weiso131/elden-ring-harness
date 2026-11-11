# elden-ring-harness

A harness that lets an AI agent investigate ELDEN RING's performance on its own:
put the game into a chosen situation, measure what happens inside it, and line that
up with system-level tools to find why frames drop and propose fixes.

It is built for being driven by an agent first: every control is a command-line
call with plain text answers (`erh`), the game's state can be polled at any time
(`erh status`), scripts run without a person at the keyboard (`launch.sh --title
--run ...`), and every recording is a set of TSV files stamped with
Linux's CLOCK_MONOTONIC_RAW (what Wine's QueryPerformanceCounter reads), so
other tools' records can be lined up with them. A person can use the same commands, and
the scripts ask for what they need when run without arguments.

What the mod does inside the game:

- times the stages of the game loop each frame (task groups), on the main thread
- sets up situations: warp to a grace, revive a boss through its flags, put it in
  its second phase, keep the player alive
- presses game actions, so a run needs no hands

## Only for experiments

The mod is never loaded in normal play. Launching from Steam goes through
`start_protected_game.exe` with Easy Anti-Cheat on and the game stays untouched.
Experiments launch through the me3 mod loader with their own profile, offline,
on a separate save file.

## Build

The DLL targets Windows; on Linux build it with
[cargo-xwin](https://github.com/rust-cross/cargo-xwin):

```sh
rustup target add x86_64-pc-windows-msvc
cargo install --locked cargo-xwin
cargo xwin build --release --target x86_64-pc-windows-msvc
```

## Run

Install [me3](https://github.com/garyttierney/me3), back up your save, then:

```sh
./launch.sh                                  # recordings in $ERH_LOG_ROOT/<date-time>/
ERH_GROUPS=FrameBegin,DrawStep ./launch.sh   # task groups to mark, or `all`
./launch.sh --run "scripts/boss.sh radahn-dlc" # run a command once the mod answers
./launch.sh --title                          # press through the title screen into the last save
```

`launch.sh` turns on MangoHud unless `MANGOHUD=0` is given, sets `ERH_OUT`, and starts me3 from outside this directory: me3 picks its
own Windows binaries from `./target/x86_64-pc-windows-msvc/release` first, where it
would find this project's build instead and the game would not start.

me3 keeps the game offline and switches to the save `ERH0000.sl2`, copied from
the current one on first launch. The mod does nothing unless `ERH_OUT` is set.

Settings for this machine go in `config` (copy `config.example`; the environment
overrides it). Recordings go to `$ERH_LOG_ROOT/<name>` (default `out/<name>`); put
that on a disk other than the game's so writing them does not compete with the
game's reads. The command channel and status stay in
`out/<name>`, which links to the recording as `recording`. launch.sh adds an empty
package under the log root to the me3 profile it generates, because the game's
Steam Linux Runtime container only sees the filesystems me3 mounts for the
profile's files.

The recording gets:

| file | columns | content |
|---|---|---|
| `meta.tsv` | `key value` | the game's Linux pid and the QueryPerformanceCounter frequency |
| `groups.tsv` | `group name` | index and name of each task group, in the order the game runs them |
| `tasks.tsv` | `t_ns tid group` | one row each time a marker task ran: QueryPerformanceCounter in ns, Linux tid, task group |

The mod registers a marker task in each task group named in `ERH_GROUPS` (default
`FrameBegin`; the names are in `groups.tsv`). Groups run in a fixed order every
frame, so the gap between two consecutive markers is roughly the time of the groups
in between; tasks inside a group have no fixed order, so a marker can sit anywhere
in its group.

Marking changes what it measures: a marker costs about 2 us, but groups the game
seems to leave empty, such as the debug ones, took about 150 us per frame once
marked. With `ERH_GROUPS=all` the game dropped from 60 to 30 fps.

## Control

While the game runs, `erh` (in `cli/`) talks to the mod through the run directory:

```sh
cargo build --release --manifest-path cli/Cargo.toml
cli/target/release/erh status        # stage, HP, block and position of the player
cli/target/release/erh ping
cli/target/release/erh nodead on     # HP stops at 1 instead of 0; `off` to undo
cli/target/release/erh warp 100001   # warp to a grace by BonfireWarpParam row id
cli/target/release/erh flag ID off   # read (no state) or set an event flag, e.g. a boss's defeated flag
cli/target/release/erh targethp 40   # set the target's HP to 40% of max: the locked-on enemy, else the
                                     # nearest of the three loaded characters with the most max HP
cli/target/release/erh targethp 1 2  # the same for the character ranked 2nd by max HP
cli/target/release/erh chars         # list the ten loaded characters with the most max HP
cli/target/release/erh nohit on      # enemy attacks do not hit the player
cli/target/release/erh lockon        # turn to the target and press lockon, retrying until it locks
cli/target/release/erh lockhp 50     # the same, then set the locked enemy's HP the frame it locks
cli/target/release/erh face          # turn the player to the target without locking on
cli/target/release/erh tp X Y Z [Q0 Q1 Q2 Q3] # move the player to a world position and facing,
                                     # as status's `world` and `facing` show them
cli/target/release/erh exterminate on # every hit by the player deals 9999999 damage
cli/target/release/erh flagdump alive # write the ids of all set event flags to flags-alive.txt
cli/target/release/erh graces       # list all graces with their unlock and cleared flags in graces.tsv
cli/target/release/erh press skill 3 # hold a key for 3 frames (default 1)
cli/target/release/erh probe attack  # for 120 frames, count when the mod sees attack held; see status
cli/target/release/erh scan          # for 120 frames, list raw input indices seen down; see status
cli/target/release/erh rawpress 252 3 # hold a raw input index, e.g. a menu key found by scan
cli/target/release/erh keypress 0x12 3 # hold a key by DirectInput code (0x12 is E) in the keyboard device
cli/target/release/erh mousepress 0 3 # hold a mouse button (0 is left) in the mouse device
cli/target/release/erh move 90 30    # push the movement stick 90 degrees right of where the camera
                                     # looks, for 30 frames
cli/target/release/erh walkto X Z [RADIUS [SECONDS]] # steer to a position in the block (status's
                                     # `pos`) until within RADIUS (default 1) meters; see walk
cli/target/release/erh stop          # drop walks, lock-ons and held keys
cli/target/release/erh give 2180000  # drop an item at the player's feet (here Carian Knight's Sword)
cli/target/release/erh equip 2180000 # put an inventory weapon in a weapon slot (default 1, right hand 1)
cli/target/release/erh asm           # list the six weapon slots
cli/target/release/erh inv [NAME]    # write the inventory and storage box to inv[-NAME].tsv
```

`erh` uses `$ERH_DIR`, `-d RUN_DIR`, or else the newest run in `./out`. A command
is a file `cmd/<id>.cmd`; the mod runs it on the game's main thread at the start of
the next frame and answers in `cmd/<id>.out`. `status.tsv` is rewritten every 50 ms.

`warp` calls the game's own warp-to-grace, found by a byte pattern because
fromsoftware-rs has no binding for it; the pattern and the call come from
[Erd-Tools](https://github.com/Nordgaren/Erd-Tools). Row ids are listed in Erd-Tools' `BonfireWarpParam.txt`, e.g. 100001 is
Margit, the Fell Omen.

`press` keys: `attack`, `strong`, `guard`, `skill`, `roll`, `jump`, `item`,
`lockon`, `action`, and the movement directions `forward`, `back`, `left`, `right`,
pushed all the way. A press goes into the same virtual input the controller and
keyboard feed, so it follows the player's key bindings. Presses only reach the game
while its window has focus, so run `erh` from somewhere that does not take it, such
as ssh from another device.

`lockon` faces the player to the target and presses once a try: with the
target out of view a press only swings the camera behind the player, who faces
it by then, so the next press locks.

Movement follows the camera, as with a real stick, so `move 0` while locked on
goes towards the enemy.

`give` drops the item on the ground like an enemy's drop, for `press action`
to pick up. It calls the game's own spawn, found by byte patterns taken from
[eldenring-practice-tool](https://github.com/veeenu/eldenring-practice-tool);
its answer shows the addresses found, which for 2.7.2 should be
`func_rva=0x55bea0 map_item_man_rva=0x3d6eae0`. The id carries the category in
its top bits (weapons 0, armor 0x10000000, talismans 0x20000000, goods
0x40000000, ashes of war 0x80000000), and a weapon's upgrade level is added to
it (2180010 is the sword at +10). Ids are listed in Erd-Tools'
`EquipParamWeapon.txt` and the like. `equip` writes the slot directly,
as the equipment menu is assumed to; not tried in the game yet.

`targethp 0` kills a two-phase boss outright instead of starting its second phase,
and `targethp 1` starts nothing (seen with Malenia). The phase change most likely
hangs off taking damage, which writing HP skips; `exterminate on` and one hit is
meant to end a phase through the damage path instead, not tried yet.

Seen in the game: DLC Radahn enters his second phase as soon as `targethp 50` is
set while the player is locked on to him, with no hit needed; without the lock-on
he does not. His grab kills on the second hit whatever `nodead` says,
which `nohit` is for.

`scripts/boss.sh` chains these to bring a defeated boss from `bosses.tsv` back,
optionally in a later phase. Run without arguments it asks what to do; with them
it runs straight through (`--help` lists the options, `--list` the bosses):

```sh
scripts/boss.sh                        # asks
scripts/boss.sh radahn-dlc             # DLC Radahn in his first phase
scripts/boss.sh radahn-dlc --phase2 --nohit # in his second
```

It clears the boss's flags, warps to Roundtable Hold and back so the boss's map is
reloaded, and waits for the boss to be loaded. With `--phase2` it then sets the
boss's HP to the table's phase HP, locking on first for bosses that change phase
only while locked on, and pressing Esc for those whose change starts a cutscene. It ends turned to the boss and
locked on, unless `--no-lock` is given.

`scripts/path.sh` records a route walked by hand and walks it again, for
bosses whose fight starts at a fog gate that warping skips:

```sh
scripts/path.sh record godrick.path  # walk from the grace to the fog, then Ctrl-C
scripts/path.sh play godrick.path    # walk it with walkto, point by point
```

`--title` presses E on a uinput keyboard (`scripts/title.py`), since the title menus
do not take the mod's presses, five seconds apart, until `status` shows the player in
the game. Like a real keyboard it needs the game window focused. It needs
`/dev/uinput` writable by the logged-in user, which Steam's `60-steam-input.rules`
already grants, and python3-libevdev. If an ibus input method other than a plain
keyboard layout is active (chewing takes E for itself), it switches ibus to
`xkb:us::eng` and leaves it there.
