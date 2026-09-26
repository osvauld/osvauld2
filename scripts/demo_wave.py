"""demo_wave — the AI's turn in the talk: drive the robotic arm on the slides deck.

    python3 scripts/demo_wave.py                          # $OSVAULD_SOCKET or /tmp/osvauld.sock
    python3 scripts/demo_wave.py --socket /path/bridge.sock --app slides

Attaches to an already-running Sthalam (it spawns nothing), finds the slides app, moves the
deck to the arm slide and plays the arm through the app's own buttons — the same handlers a
person's click runs, not a doc write behind the app's back. The Sthalam window must be visible:
a hidden window's event loop stalls and every request here times out.
"""

import argparse
import os
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from osvauld.client import Bridge


def find_item(rpc: Bridge, name: str) -> str:
    for ws in rpc.list_workspaces():
        for item in rpc.list_items(ws["id"]):
            if item["name"] == name:
                return item["id"]
    raise SystemExit(f"no app named {name!r} — upload the deck first")


def go_to_slide(rpc: Bridge, item: str, slide: str) -> None:
    # The deck exposes no "jump" action, so rewind with its own prev button and step forward.
    for _ in range(20):
        rpc.click(item, "prev")
    for _ in range(20):
        if f"slide:{slide}" in str(rpc.dump_tree(item)):
            return
        rpc.click(item, "next")
    raise SystemExit(f"slide {slide!r} is not in this deck")


def say(line: str) -> None:
    print(f"agent: {line}", flush=True)


def main() -> None:
    ap = argparse.ArgumentParser(description="drive the arm slide over the bridge")
    ap.add_argument("--socket", default=os.environ.get("OSVAULD_SOCKET", "/tmp/osvauld.sock"))
    ap.add_argument("--app", default="slides", help="the deck's item name")
    args = ap.parse_args()

    rpc = Bridge(args.socket, timeout=90)
    item = find_item(rpc, args.app)

    say("opening the arm slide")
    go_to_slide(rpc, item, "arm")
    time.sleep(1)

    say("waving")
    rpc.click(item, "wave")
    time.sleep(4)
    rpc.click(item, "wave")

    say("raising the shoulder, one step at a time")
    for _ in range(8):
        rpc.click(item, "inc:shoulder")
        time.sleep(0.25)
    say("bending the elbow")
    for _ in range(6):
        rpc.click(item, "dec:elbow")
        time.sleep(0.25)
    time.sleep(1.5)

    say("back to rest")
    rpc.click(item, "reset")
    errors = rpc.read_console(item)
    say("done, console clean" if not errors else f"done, console says: {errors}")


if __name__ == "__main__":
    main()
