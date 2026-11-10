"""SVG drawings on the real path (plan: docs/design/svg-drawings.md §0, S-V1..S-V3): upload
Hillclimb with its art/*.svg files, see the SVG wheels drawn, then change one colour in
wheel.svg over the bridge (read_file_versioned + exact edit_file) and see the pixels change."""

import base64
import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from osvauld.png import Image, near
from osvauld.session import Session, shell_binary

ROOT = Path(__file__).resolve().parent.parent
SHOTS = Path(os.environ.get("HILLCLIMB_SHOTS", "/tmp"))
SPOKES = "#7d8590"  # wheel.svg's spokes
MARKER = "#ff00ff"  # nothing else in the game is magenta


def count(shot, colour):
    n = 0
    for y in range(0, shot.height, 2):
        for x in range(0, shot.width, 2):
            if near(shot.at(x, y), colour, 30):
                n += 1
    return n


def main():
    with Session(shell_binary=shell_binary(), offscreen=(900, 700)) as s:
        rpc = s.rpc
        rpc.signup("artist", "artist passphrase")
        ws = rpc.create_workspace("art")
        item = rpc.create_item(ws["id"], "Hillclimb", "app")["id"]

        # S-V1: the upload carries the .svg files, and the app reads them with no error.
        rpc.upload_folder(item, ROOT / "demo_apps" / "hillclimb")
        assert "<svg" in rpc.read_file_versioned(item, "art/wheel.svg")["content"]
        rpc.open_item(item)
        rpc.frame(120)
        console = rpc.read_console(item)
        assert not console, console

        # S-V2: the wheels are drawn from the SVG.
        before = Image(base64.b64decode(rpc.screenshot(item)["png_base64"]))
        rpc.save_screenshot(item, SHOTS / "svg_wheels.png")
        assert count(before, MARKER) == 0, "magenta before the edit"

        # S-V3: one exact edit to the SVG over the bridge reloads the app with the new colour.
        src = rpc.read_file_versioned(item, "art/wheel.svg")
        old = f'id="spokes" fill="{SPOKES}"'
        assert src["content"].count(old) == 1, "the spokes' fill, once"
        rpc.edit_file(item, "art/wheel.svg", src["revision"], [{"old_text": old, "new_text": f'id="spokes" fill="{MARKER}"'}])
        rpc.frame(120)
        console = rpc.read_console(item)
        assert not console, console
        after = Image(base64.b64decode(rpc.screenshot(item)["png_base64"]))
        rpc.save_screenshot(item, SHOTS / "svg_wheels_edited.png")
        magenta = count(after, MARKER)
        assert magenta > 10, ("the edited spokes show", magenta)
    print(f"svg smoke ok (magenta samples after edit: {magenta})")


if __name__ == "__main__":
    main()
