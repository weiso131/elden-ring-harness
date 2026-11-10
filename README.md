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
./launch.sh out
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

The mod registers one marker task in each of the 169 task groups. Groups run in a
fixed order every frame, so the gap between two consecutive markers is roughly the
time of one group; tasks inside a group have no fixed order, so a marker can sit
anywhere in its group.
