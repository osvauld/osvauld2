"""deck_upload — push the slides deck and its embedded demo apps into a running Sthalam.

    python3 scripts/deck_upload.py                        # $OSVAULD_SOCKET or /tmp/osvauld.sock
    python3 scripts/deck_upload.py --socket /path/bridge.sock --app slides
    python3 scripts/deck_upload.py --account talk --passphrase '…'   # fresh or locked store

Attaches to an already-running Sthalam; with --account/--passphrase it signs up (empty store) or
unlocks, and creates a "Lightning talk" workspace if there is none. Finds the app named --app (creating it in the
first workspace if missing), writes every file of demo_apps/slides, then mounts each demo under
demos/<name>/. `require` resolves from the app root, so a demo's `require("theme")` would load
the deck's theme: its requires are rewritten to `require("demos/<name>/theme")` on the way up.
The repo's demo folders are never modified. Every app in STANDALONE is also uploaded unmodified as its own
app, to open in its own tab after the deck.
"""

import argparse
import os
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from osvauld.client import Bridge, BridgeError

ROOT = Path(__file__).parent.parent
DEMOS = ["linkage", "model_viewer", "dashboard", "kanban", "math_mela"]
# Opened in their own tabs only — not embedded, so their key handlers and ids can't clash.
STANDALONE = DEMOS + [
    "frame_orbits", "orbit", "voronoi", "pie", "line_chart", "node_graph", "pomodoro",
    "khoj", "bhasha_steps", "bharat_compass", "india_time_together", "tank",
]
REQUIRE = re.compile(r'require\("([^"]+)"\)')


def find_or_create(rpc: Bridge, name: str) -> str:
    workspaces = rpc.list_workspaces()
    for ws in workspaces:
        for item in rpc.list_items(ws["id"]):
            if item["name"] == name:
                return item["id"]
    if not workspaces:
        raise SystemExit("no workspace — create one in Sthalam first")
    return rpc.create_item(workspaces[0]["id"], name, "app")["id"]


def ensure_session(rpc: Bridge, account: str | None, passphrase: str | None) -> None:
    try:
        rpc.list_workspaces()
        unlocked = True
    except BridgeError:
        unlocked = False
    if not unlocked:
        if not (account and passphrase):
            raise SystemExit("Sthalam is locked — pass --account and --passphrase")
        if rpc.list_accounts():
            rpc.unlock(account, passphrase)
        else:
            rpc.signup(account, passphrase)  # the recovery phrase is not needed for a demo store
            print(f"signed up {account!r}")
    if not rpc.list_workspaces():
        rpc.create_workspace("Lightning talk")


def lua_files(folder: Path):
    # main3.lua and test.lua are kanban scratch files, not part of the app.
    return sorted(p for p in folder.rglob("*.lua") if p.name not in ("main3.lua", "test.lua"))


def main() -> None:
    ap = argparse.ArgumentParser(description="upload the deck with its demos mounted")
    ap.add_argument("--socket", default=os.environ.get("OSVAULD_SOCKET", "/tmp/osvauld.sock"))
    ap.add_argument("--app", default="slides")
    ap.add_argument("--account")
    ap.add_argument("--passphrase", default=os.environ.get("OSVAULD_PASSPHRASE"))
    args = ap.parse_args()

    rpc = Bridge(args.socket, timeout=90)
    ensure_session(rpc, args.account, args.passphrase)
    item = find_or_create(rpc, args.app)
    # The console keeps history across reloads; report only what this upload adds. It exists only
    # for an open app, and a brand-new app has no source to open yet.
    try:
        rpc.open_item(item)
        seen = len(rpc.read_console(item))
    except BridgeError:
        seen = 0

    # Demos first: the deck's content.lua names them, so they must exist before it reloads.
    for name in DEMOS:
        src = ROOT / "demo_apps" / name
        for path in lua_files(src):
            body = REQUIRE.sub(rf'require("demos/{name}/\1")', path.read_text())
            rpc.write_file(item, f"demos/{name}/{path.relative_to(src).as_posix()}", body)
        print(f"mounted demos/{name}")

    # Every demo as a standalone app, for opening in its own tab after the deck.
    for name in STANDALONE:
        src = ROOT / "demo_apps" / name
        own = find_or_create(rpc, name)
        for path in [*lua_files(src), src / "manifest.osv"]:
            if path.exists():
                rpc.write_file(own, path.relative_to(src).as_posix(), path.read_text())
        print(f"standalone {name}")

    deck = ROOT / "demo_apps" / "slides"
    for path in [*lua_files(deck), deck / "manifest.osv"]:
        rpc.write_file(item, path.relative_to(deck).as_posix(), path.read_text())
    print("deck uploaded")
    rpc.open_item(item)

    errors = rpc.read_console(item)[seen:]
    print("console clean" if not errors else f"console: {errors}")


if __name__ == "__main__":
    main()
