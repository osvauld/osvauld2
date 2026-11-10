"""Hillclimb maps on the real path (plan: docs/design/hillclimb-maps.md §0, S-M1..S-M3): the
default course is the hand-written maps/green_hills.lua with SVG scenery; `Map:` switches to the
seeded one and back. An agent's edit to the map file over the bridge reshapes the ground live,
and a broken edit names the bad point on the console until it is fixed."""

import os
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from osvauld.session import Session, shell_binary
from smoke_hillclimb import Game, texts

ROOT = Path(__file__).resolve().parent.parent
SHOTS = Path(os.environ.get("HILLCLIMB_SHOTS", "/tmp"))
MAP = "maps/green_hills.lua"
TOP = re.compile(r"(\t*)([\d.]+), ([\d.-]+),( *-- the top.*)")  # the map's highest control point


def button(tree, prefix):
    return next((t for t in texts(tree) if t.startswith(prefix)), None)


def settled(g, logged=0):
    """Run 2 s; nothing new on the console (it keeps `logged` old lines: a reload doesn't clear it)."""
    g.rpc.frame(240)
    console = g.rpc.read_console(g.item)
    assert len(console) == logged, console
    ents, world, tree = g.look()
    c = ents["chassis"]
    assert abs(c["rot"]) < 3 and abs(c["velocity"][0]) < 5, ("at rest on the start", c)
    return ents, world, tree


def main():
    with Session(shell_binary=shell_binary(), offscreen=(900, 700)) as s:
        g = Game(s)
        rpc = g.rpc

        # S-M1: the hand-written map is the default, with its scenery; Map: cycles.
        ents, world, tree = settled(g)
        assert button(tree, "Map: ") == "Map: Green Hills", texts(tree)
        src = rpc.read_file_versioned(g.item, MAP)
        pieces = len(re.findall(r'\{ "art/[\w/]+\.svg"', src["content"]))
        scenery = [i for i in ents if i.startswith("scenery")]
        assert pieces >= 5 and len(scenery) == pieces, ("every scenery piece is an entity", pieces, scenery)
        rpc.save_screenshot(g.item, SHOTS / "maps_green_hills.png")
        rpc.click(g.item, "swap_map")
        ents, world, tree = settled(g)
        assert button(tree, "Map: ") == "Map: Seed 7", texts(tree)
        assert not [i for i in ents if i.startswith("scenery")], "the seeded map has no scenery"
        rpc.click(g.item, "swap_map")
        ents, world, tree = settled(g)
        assert button(tree, "Map: ") == "Map: Green Hills", texts(tree)
        print("  S-M1: Green Hills by default,", pieces, "scenery pieces; Map: cycles")

        # S-M2: raise the highest control point 5 m; the ground's top follows exactly.
        before = ents["ground"]["pos"][1]
        src = rpc.read_file_versioned(g.item, MAP)
        found = TOP.search(src["content"])
        assert found, "the map marks its highest point '-- the top'"
        old = found.group(0)
        tabs, x, h, rest = found.groups()
        new = f"{tabs}{x}, {float(h) + 5:g},{rest}"
        rpc.edit_file(g.item, MAP, src["revision"], [{"old_text": old, "new_text": new}])
        ents, world, tree = settled(g)
        after = ents["ground"]["pos"][1]
        assert abs((before - after) - 500) < 1e-6, ("the ground's top rose 5 m", before, after)
        rpc.save_screenshot(g.item, SHOTS / "maps_raised.png")
        print("  S-M2: the edit raised the ground's top by", before - after, "units")

        # S-M3: a broken edit names the point; the fix brings the car back.
        src = rpc.read_file_versioned(g.item, MAP)
        broken = f"{tabs}0.5, {float(h) + 5:g},{rest}"
        rpc.edit_file(g.item, MAP, src["revision"], [{"old_text": new, "new_text": broken}])
        rpc.frame(30)
        console = rpc.read_console(g.item)
        said = " ".join(str(line) for line in console)
        assert "x 0.5 m" in said and "must be right of point" in said, console
        src = rpc.read_file_versioned(g.item, MAP)
        rpc.edit_file(g.item, MAP, src["revision"], [{"old_text": broken, "new_text": old}])
        ents, world, tree = settled(g, logged=len(console))
        assert "chassis" in ents and abs(ents["ground"]["pos"][1] - before) < 1e-6, ("fixed: back as it was", before, ents["ground"]["pos"])
        print("  S-M3: the bad point was named; the fix restored the map")


if __name__ == "__main__":
    main()
    print("hillclimb maps: ok")
