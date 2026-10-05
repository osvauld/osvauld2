"""The asset editor over the real path (design/maker-parity.md §0): its point and part drags by
the pointer (E3); a room made in it plays as a map, its entities taking their world fields from
their assets and the room's own on top (E1); a UI asset it made is a widget in another app (E2)."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from osvauld.session import Session, shell_binary


def find(tree, el_id):
    if tree.get("id") == el_id:
        return tree
    for child in tree.get("children", []):
        found = find(child, el_id)
        if found:
            return found
    return None


with Session(shell_binary=shell_binary(), offscreen=(1200, 900)) as s:
    s.rpc.signup("maker", "maker passphrase")
    ws = s.rpc.create_workspace("maker smoke")
    item = s.rpc.create_item(ws["id"], "asset editor", "app")["id"]
    s.rpc.upload_folder(item, Path(__file__).parent.parent / "demo_apps" / "asset_editor")
    s.rpc.open_item(item)
    s.rpc.frame(2)
    assert not s.rpc.read_console(item), s.rpc.read_console(item)

    # E3: the editor's gestures through the real pointer. The canvas draws the drawing at 2.5×
    # from its top-left and the camera starts unzoomed, so a drawing point is found, not guessed.
    ZOOM = 2.5
    canvas = next(r for r in s.rpc.rects() if r["id"] == "canvas")

    def screen(x, y):
        return canvas["x"] + x * ZOOM, canvas["y"] + y * ZOOM

    def hero():
        return s.rpc.read_data(item)["asset:hero"]

    def part(a, pid):
        return next(p for p in a["views"]["front"]["parts"] if p["id"] == pid)

    # Select arm_far, then drag the hand's cubic end — 8 px from its controls at 1×, so this
    # holds only because ends draw, and so hit, above controls — by (25, 7.5) on screen: (10, 3)
    # in the drawing, already on the 1× snap. The end and the controls either side of it move by
    # that; every other number in the path stays.
    s.rpc.click(item, "part:arm_far")
    s.rpc.frame(2)
    before = hero()
    end = part(before, "arm_far")["shapes"][1]["path"][1][5:7]
    s.rpc.drag(screen(*end), (screen(*end)[0] + 25, screen(*end)[1] + 7.5))
    s.rpc.frame(2)
    after = hero()
    path0, path1 = (part(a, "arm_far")["shapes"][1]["path"] for a in (before, after))
    moved = {(1, 3), (1, 4), (1, 5), (1, 6), (2, 1), (2, 2)}  # (command, number) pairs
    for j, (was, now) in enumerate(zip(path0, path1)):
        for k in range(1, len(was)):
            d = (10 if k % 2 else 3) if (j, k) in moved else 0
            assert abs(now[k] - was[k] - d) < 1e-6, f"command {j} number {k}: {was[k]} → {now[k]}"
    assert part(after, "head") == part(before, "head"), "a point drag touches only its part"

    # Drag the head by a point on its face, clear of any dot: it moves by its snapped step, and
    # the hat socket on it comes along.
    head0, socket0 = part(after, "head")["pivot"], after["sockets"]["head_top"]["at"]
    s.rpc.drag(screen(80, 70), (screen(80, 70)[0] + 25, screen(80, 70)[1] + 10))
    s.rpc.frame(2)
    moved = hero()
    assert part(moved, "head")["pivot"] == [head0[0] + 10, head0[1] + 4], part(moved, "head")["pivot"]
    assert moved["sockets"]["head_top"]["at"] == [socket0[0] + 10, socket0[1] + 4], moved["sockets"]
    assert part(moved, "body") == part(after, "body"), "the body is above the head, so it stays"
    assert not s.rpc.read_console(item), s.rpc.read_console(item)

    def entities():
        return {e["id"]: e for e in find(s.rpc.dump_tree(item), "stage-play")["world"]["entities"]}

    def hold(code, key, frames):
        s.rpc.keyboard(code, key, True)
        s.rpc.frame(frames)
        s.rpc.keyboard(code, key, False)
        s.rpc.frame(2)

    # E1: play the room. The chest is solid by its asset, the crate loose by its own.
    s.rpc.click(item, "asset:room")
    s.rpc.frame(3)  # the room seeds, then the assets it shows
    s.rpc.click(item, "stage:play")
    s.rpc.frame(2)
    assert not s.rpc.read_console(item), s.rpc.read_console(item)
    seen = entities()
    assert (seen["hero"]["body"], seen["chest"]["body"], seen["crate"]["body"]) == ("moved", "fixed", "loose"), seen

    # Held D drives the hero right; its foot circle (r 16, at x 80) stops at the chest's
    # collider (x 8 into a chest at 460): pos.x 372, far short of where 3 s would take it.
    hold("KeyD", "d", 180)
    x = entities()["hero"]["pos"][0]
    assert abs(x - 372) < 2, f"the hero should stop at the chest, not at x {x}"

    # Held A drives it back across the room into the crate, which slides away.
    crate = entities()["crate"]["pos"]
    hold("KeyA", "a", 240)
    moved = entities()["crate"]["pos"]
    assert moved[0] < crate[0] - 10, f"the hero should push the crate: {crate} → {moved}"
    assert not s.rpc.read_console(item), s.rpc.read_console(item)

    # E2: the toggle the editor seeded, as a widget in another app — a plain frame posed by
    # `clip:at`, no world. A click plays `to_night` (0.6 s) and rests on night; another goes back.
    s.rpc.click(item, "asset:toggle")  # the editor seeds it, then saves it as a workspace asset
    s.rpc.frame(3)
    demo = s.rpc.create_item(ws["id"], "toggle demo", "app")["id"]
    s.rpc.upload_folder(demo, Path(__file__).parent.parent / "demo_apps" / "toggle_demo")
    s.rpc.open_item(demo)
    s.rpc.frame(3)
    assert not s.rpc.read_console(demo), s.rpc.read_console(demo)

    def readout():
        def walk(t):
            text = str(t.get("text", ""))
            if text.startswith("toggle: "):
                return text[len("toggle: "):]
            return next((r for r in map(walk, t.get("children", [])) if r), None)
        return walk(s.rpc.dump_tree(demo))

    def worlds(t):
        return ("world" in t) + sum(worlds(c) for c in t.get("children", []))

    assert readout() == "day", readout()
    for to in ("night", "day"):
        s.rpc.click(demo, "toggle")
        s.rpc.frame(6)
        assert readout() == "to_" + to, readout()
        s.rpc.frame(40)
        assert readout() == to, readout()
    assert worlds(s.rpc.dump_tree(demo)) == 0, "the toggle should need no world"
    assert not s.rpc.read_console(demo), s.rpc.read_console(demo)

    print("asset editor: a point and a part drag by the pointer, the socket riding along; a room "
          "made in the editor plays — a solid chest, a pushed crate; the toggle it made switches "
          "in another app, without a world")
