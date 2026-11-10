# elden-ring-harness

An ELDEN RING mod that exposes the game's internals to experiment scripts: time
what the main thread spends each frame, warp to a fixed starting point, reset a
boss, and replay a recorded route, so frame drops can be reproduced and explained
from inside the game.

Status: research only, no code yet. See [docs/research.md](docs/research.md).

## Only for experiments

The mod is never loaded in normal play. Launching from Steam goes through
`start_protected_game.exe` with Easy Anti-Cheat on and the game stays untouched.
Experiments launch through the me3 mod loader with their own profile, offline,
on a separate save file.
