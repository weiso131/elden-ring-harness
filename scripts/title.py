#!/usr/bin/env python3
"""Gets from the title screen into the game by pressing E on a virtual keyboard.

The mod's presses do not get through the title menus, so this goes through
the kernel's uinput like a real keyboard; like one, it only reaches the game
while its window has focus. /dev/uinput is writable by the logged-in user via
the uaccess rule Steam installs (60-steam-input.rules), so no root is needed.

Usage: scripts/title.py [--first SECONDS] [--gap SECONDS] [--tries N]
Stops as soon as `erh status` reports the player in the game.
"""
import argparse
import os
import subprocess
import sys
import time

import libevdev

ERH = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "cli", "target", "release", "erh")


def ibus_engine(name=None):
    """Gets, or sets with `name`, the ibus engine; None if ibus is not there."""
    try:
        r = subprocess.run(["ibus", "engine"] + ([name] if name else []),
                           capture_output=True, text=True, timeout=5)
    except (OSError, subprocess.TimeoutExpired):
        return None
    return r.stdout.strip() if r.returncode == 0 else None


def in_game():
    out = subprocess.run([ERH, "status"], capture_output=True, text=True).stdout
    return any(line == "stage\tin_game" for line in out.splitlines())


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--first", type=float, default=15, help="wait before the first press (title screen loading)")
    # Presses closer together than this were not safe through the confirmations.
    p.add_argument("--gap", type=float, default=5, help="seconds between presses")
    p.add_argument("--tries", type=int, default=6, help="presses before giving up")
    p.add_argument("--engine", default="xkb:us::eng", help="ibus engine to use while pressing")
    a = p.parse_args()

    d = libevdev.Device()
    d.name = "elden-ring-harness keyboard"
    d.enable(libevdev.EV_KEY.KEY_E)
    try:
        kb = d.create_uinput_device()
    except OSError as e:
        sys.exit(f"cannot create a uinput device ({e}); is /dev/uinput writable?")
    # The compositor needs a moment to start listening to a new device.
    time.sleep(1)

    time.sleep(a.first)
    # A Chinese input method such as chewing takes E for itself, so switch to
    # plain US keys; the player does not need the old one back.
    engine = ibus_engine()
    if engine and not engine.startswith("xkb:"):
        if ibus_engine(a.engine) is None:
            print(f"could not switch ibus from {engine} to {a.engine}", file=sys.stderr)
        else:
            print(f"ibus {engine} -> {a.engine}", flush=True)
    press(kb, a)


def press(kb, a):
    for i in range(a.tries):
        if in_game():
            print(f"in game after {i} presses")
            return
        kb.send_events([libevdev.InputEvent(libevdev.EV_KEY.KEY_E, 1), libevdev.InputEvent(libevdev.EV_SYN.SYN_REPORT, 0)])
        time.sleep(0.1)
        kb.send_events([libevdev.InputEvent(libevdev.EV_KEY.KEY_E, 0), libevdev.InputEvent(libevdev.EV_SYN.SYN_REPORT, 0)])
        print(f"pressed E ({i + 1})", flush=True)
        time.sleep(a.gap)
    for _ in range(30):
        if in_game():
            print("in game")
            return
        time.sleep(1)
    sys.exit("still not in the game; does the game window have focus?")


if __name__ == "__main__":
    main()
