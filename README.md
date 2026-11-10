# elden-ring-harness

An ELDEN RING mod that exposes the game's internals to experiment scripts: time
what the main thread spends each frame, warp to a fixed starting point, reset a
boss, and replay a recorded route, so frame drops can be reproduced and explained
from inside the game.

Status: the first step, timing the game loop's task groups, builds but has not
run in the game yet. Notes on what the mod builds on are in
[docs/research.md](docs/research.md).

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
./launch.sh                                  # writes to out/<date-time>/
ERH_GROUPS=FrameBegin,DrawStep ./launch.sh   # task groups to mark, or `all`
```

`launch.sh` sets `ERH_OUT` and starts me3 from outside this directory: me3 picks its
own Windows binaries from `./target/x86_64-pc-windows-msvc/release` first, where it
would find this project's build instead and the game would not start.

me3 keeps the game offline and switches to the save `ERH0000.sl2`, copied from
the current one on first launch. The mod does nothing unless `ERH_OUT` is set.

`out/` gets:

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
```

`erh` uses `$ERH_DIR`, `-d RUN_DIR`, or else the newest run in `./out`. A command
is a file `cmd/<id>.cmd`; the mod runs it on the game's main thread at the start of
the next frame and answers in `cmd/<id>.out`. `status.tsv` is rewritten every 50 ms.
