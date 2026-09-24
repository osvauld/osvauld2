"""Real Lua tank: keyboard movement, auto-fire, chasing spawns and swept hits."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from osvauld.session import Session, shell_binary


def readout(tree, prefix):
    if isinstance(tree, dict):
        label = tree.get("text", "")
        if label.startswith(prefix):
            return label[len(prefix):]
        for child in tree.values():
            found = readout(child, prefix)
            if found is not None:
                return found
    elif isinstance(tree, list):
        for child in tree:
            found = readout(child, prefix)
            if found is not None:
                return found
    return None


def position(tree):
    return tuple(map(int, readout(tree, "tank: ").split(",")))


with Session(shell_binary=shell_binary(), offscreen=(900, 700)) as s:
    s.rpc.signup("tank", "tank passphrase")
    ws = s.rpc.create_workspace("game smoke")
    item = s.rpc.create_item(ws["id"], "tank", "app")["id"]
    s.rpc.upload_folder(item, Path(__file__).parent.parent / "demo_apps" / "tank")
    s.rpc.open_item(item)
    assert position(s.rpc.dump_tree(item)) == (360, 240), s.rpc.read_console(item)
    pursuer = int(readout(s.rpc.dump_tree(item), "enemy: ").split(",")[0])
    s.rpc.frame(8)
    assert int(readout(s.rpc.dump_tree(item), "enemy: ").split(",")[0]) < pursuer, \
        "enemy did not pursue the stationary tank"
    before = s.rpc.screenshot(item)["png_base64"]
    arena = next(r for r in s.rpc.rects() if r["id"] == "arena")
    assert "hover" in arena["hits"] and "click" not in arena["hits"], arena
    # Target the arena in its own coordinates; no hard-coded screen coordinates.
    s.rpc.move_to(arena["x"] + 480, arena["y"] + 240)
    assert readout(s.rpc.dump_tree(item), "aim: ") == "90"
    right_pixels = s.rpc.screenshot(item)["png_base64"]
    before_right_score = int(readout(s.rpc.dump_tree(item), "score: "))
    s.rpc.frame(24)  # no click: aiming is enough to start the timed stream
    live = int(readout(s.rpc.dump_tree(item), "shots: ").split("/")[0])
    assert 0 < live <= 32, (live, s.rpc.read_console(item))
    fired_pixels = s.rpc.screenshot(item)["png_base64"]
    s.rpc.frame(6)
    assert s.rpc.screenshot(item)["png_base64"] != fired_pixels, "shots did not move in pixels"
    s.rpc.advance(0.1)  # a larger step still checks the whole travelled segment
    s.rpc.frame(120)
    score = readout(s.rpc.dump_tree(item), "score: ")
    assert int(score) > before_right_score, (before_right_score, score, s.rpc.read_console(item))
    population = readout(s.rpc.dump_tree(item), "enemies: ")
    assert int(population.split(" · spawned: ")[1]) > 3, population
    assert int(population.split("/")[0]) <= 12, population
    s.rpc.frame(120)
    assert readout(s.rpc.dump_tree(item), "score: ") == score, "dead enemy scored twice"
    s.rpc.move_to(arena["x"] + 360, arena["y"] + 90)
    s.rpc.frame(60)
    assert int(readout(s.rpc.dump_tree(item), "score: ")) >= int(score), "score regressed"
    s.rpc.move_to(arena["x"] + 360, arena["y"] + 330)
    assert readout(s.rpc.dump_tree(item), "aim: ") == "180"
    assert s.rpc.screenshot(item)["png_base64"] != right_pixels
    s.rpc.move_to(arena["x"] - 8, arena["y"] + 240)
    assert readout(s.rpc.dump_tree(item), "aim: ") == "180", "leaving lost the last aim"
    s.rpc.keyboard("KeyD", "d", True)
    s.rpc.frame(3)
    assert readout(s.rpc.dump_tree(item), "aim: ") == "180", "last aim followed a departed pointer"
    s.rpc.keyboard("KeyD", "d", False)

    s.rpc.move_to(arena["x"] + 480, arena["y"] + 150)
    initial_aim = int(readout(s.rpc.dump_tree(item), "aim: "))
    s.rpc.keyboard("KeyD", "q", True)  # physical key, even on a non-QWERTY layout
    s.rpc.frame(24)
    moved = s.rpc.dump_tree(item)
    east = position(moved)
    assert east[0] > 390 and east[1] == 240, east
    assert int(readout(moved, "aim: ")) < initial_aim, "turret did not track a still pointer"
    s.rpc.keyboard("KeyD", "q", False)
    s.rpc.frame(2)
    stopped = position(s.rpc.dump_tree(item))
    s.rpc.frame(5)
    assert position(s.rpc.dump_tree(item)) == stopped, "tank kept moving after key-up"
    assert s.rpc.screenshot(item)["png_base64"] != before, "motion did not reach pixels"

    s.rpc.keyboard("KeyA", "a", True)
    s.rpc.keyboard("ArrowUp", "ArrowUp", True)
    start = position(s.rpc.dump_tree(item))
    s.rpc.frame(20)
    end = position(s.rpc.dump_tree(item))
    assert end[0] < start[0] and end[1] < start[1], (start, end)
    assert abs((start[0] - end[0]) - (start[1] - end[1])) < 8, (start, end)
    s.rpc.keyboard("KeyA", "a", False)
    s.rpc.keyboard("ArrowUp", "ArrowUp", False)

    s.rpc.keyboard("KeyD", "d", True)
    s.rpc.frame(300)
    assert position(s.rpc.dump_tree(item))[0] == 698, "tank escaped the right edge"
    s.rpc.keyboard("KeyD", "d", False)
    assert s.rpc.read_console(item) == []

print("tank game smoke ok")
