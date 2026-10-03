"""Table hockey: the faceoff holds the puck bodiless for its drop, the centre line stops a paddle
but not the puck, a goal scores and puts the puck back on the spot. Read from the dump, not pixels."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from osvauld.session import Session, shell_binary


def texts(tree):
    found = [tree["text"]] if isinstance(tree.get("text"), str) else []
    for child in tree.get("children", []):
        found += texts(child)
    return found


def rink(tree):
    if tree.get("id") == "rink":
        return tree
    for child in tree.get("children", []):
        found = rink(child)
        if found:
            return found
    return None


def world(tree):
    return {e["id"]: e for e in rink(tree)["world"]["entities"]}


with Session(shell_binary=shell_binary(), offscreen=(1100, 760)) as s:
    s.rpc.signup("hockey", "hockey passphrase")
    ws = s.rpc.create_workspace("hockey smoke")
    item = s.rpc.create_item(ws["id"], "hockey", "app")["id"]
    s.rpc.upload_folder(item, Path(__file__).parent.parent / "demo_apps" / "hockey")
    s.rpc.open_item(item)
    s.rpc.frame(2)
    assert not s.rpc.read_console(item), s.rpc.read_console(item)

    def look():
        tree = s.rpc.dump_tree(item)
        return world(tree), texts(tree)

    def hold(code, frames):
        s.rpc.keyboard(code, code, True)
        s.rpc.frame(frames)
        s.rpc.keyboard(code, code, False)
        s.rpc.frame(2)

    # The faceoff: the puck drops for a second with no body, then is in play.
    seen, said = look()
    assert seen["puck"]["body"] == "none" and "Faceoff" in said and "0  :  0" in said, (seen, said)
    s.rpc.advance(1.1)
    s.rpc.frame(2)
    seen, said = look()
    assert seen["puck"]["body"] == "loose" and "Faceoff" not in said, (seen["puck"], said)

    # Red steps out of the way; blue runs at the puck. Blue stops at the centre line (its right
    # edge at 447, the line at 448); the puck crosses and goes on into the right goal.
    hold("ArrowUp", 60)
    # Frame by frame, to catch the squash the puck plays when blue hits it (on_hit, 0.15 s).
    s.rpc.keyboard("KeyD", "KeyD", True)
    squashed = False
    for _ in range(40):
        s.rpc.frame(1)
        clip = look()[0]["puck"]["clip"]
        squashed = squashed or (clip is not None and abs(clip["length"] - 0.15) < 1e-6)
    s.rpc.keyboard("KeyD", "KeyD", False)
    s.rpc.frame(2)
    assert squashed, "the puck squashed when blue hit it"
    seen, _ = look()
    assert abs(seen["blue"]["pos"][0] + 56 - 448) < 2, ("blue stopped at the line", seen["blue"])
    assert seen["puck"]["pos"][0] > 450, ("the puck crossed it", seen["puck"])
    # Struck, it is fast: watch for the goal rather than wait a guess at how long it takes.
    for _ in range(90):
        s.rpc.frame(1)
        seen, said = look()
        if any(t.startswith("GOAL") for t in said):
            break
    assert "1  :  0" in said and "GOAL! Blue scores" in said, said
    # The puck sits in the net while the goal shows; Rust holds the faceoff timer.
    timers = s.rpc.dump_tree(item)
    timers = [t["name"] for t in rink(timers)["world"]["timers"]]
    assert timers == ["faceoff"] and seen["puck"]["pos"][0] > 800, (timers, seen["puck"])
    s.rpc.advance(1.6)
    s.rpc.frame(2)
    seen, said = look()
    assert seen["puck"]["body"] == "none" and seen["puck"]["pos"] == [434, 234], ("back on the spot", seen)
    s.rpc.advance(1.1)
    s.rpc.frame(2)
    seen, said = look()
    assert seen["puck"]["body"] == "loose" and not any(t.startswith("GOAL") for t in said), said
    assert not s.rpc.read_console(item), s.rpc.read_console(item)

    # Blue still stands at the line, over the spot's edge: the puck came down in it and is
    # pushed clear, not left asleep inside the paddle.
    s.rpc.frame(60)
    seen, _ = look()
    assert seen["puck"]["pos"][0] >= seen["blue"]["pos"][0] + 56 - 1, ("pushed out of blue", seen)
    if len(sys.argv) > 1:
        s.rpc.save_screenshot(item, sys.argv[1])
    print("hockey: faceoff, a paddle stopped at the centre line, the puck across it, a hit squashed it, a goal scored and the puck back on the spot")
