"""Table hockey: the faceoff holds the puck bodiless for its drop, the centre line stops a paddle
but not the puck, a goal scores and puts a fresh puck on the spot. Read from the dump, not pixels."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from osvauld.session import Session, shell_binary


def texts(tree):
    found = [tree["text"]] if isinstance(tree.get("text"), str) else []
    for child in tree.get("children", []):
        found += texts(child)
    return found


def world(tree):
    if tree.get("id") == "rink":
        return {e["id"]: e for e in tree["world"]["entities"]}
    for child in tree.get("children", []):
        found = world(child)
        if found:
            return found
    return None


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
    assert seen["puck:1"]["body"] == "none" and "Faceoff" in said and "0  :  0" in said, (seen, said)
    s.rpc.advance(1.1)
    s.rpc.frame(2)
    seen, said = look()
    assert seen["puck:1"]["body"] == "loose" and "Faceoff" not in said, (seen["puck:1"], said)

    # Red steps out of the way; blue runs at the puck. Blue stops at the centre line (its right
    # edge at 447, the line at 448); the puck crosses and goes on into the right goal.
    hold("ArrowUp", 60)
    hold("KeyD", 40)
    seen, _ = look()
    assert abs(seen["blue"]["pos"][0] + 56 - 448) < 2, ("blue stopped at the line", seen["blue"])
    assert seen["puck:1"]["pos"][0] > 450, ("the puck crossed it", seen["puck:1"])
    s.rpc.frame(90)
    seen, said = look()
    assert "1  :  0" in said and "GOAL! Blue scores" in said, said
    assert "puck:1" not in seen and seen["puck:2"]["body"] == "none", ("a fresh puck drops", seen)
    s.rpc.advance(1.1)
    s.rpc.frame(2)
    seen, said = look()
    assert seen["puck:2"]["body"] == "loose" and not any(t.startswith("GOAL") for t in said), said
    assert not s.rpc.read_console(item), s.rpc.read_console(item)

    # Blue still stands at the line, over the spot's edge: the new puck came down in it and is
    # pushed clear, not left asleep inside the paddle.
    s.rpc.frame(60)
    seen, _ = look()
    assert seen["puck:2"]["pos"][0] >= seen["blue"]["pos"][0] + 56 - 1, ("pushed out of blue", seen)
    if len(sys.argv) > 1:
        s.rpc.save_screenshot(item, sys.argv[1])
    print("hockey: faceoff, a paddle stopped at the centre line, the puck across it, a goal scored and a fresh puck")
