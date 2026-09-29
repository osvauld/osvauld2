"""A retained ui.world: entities spawn and despawn by id, hit by id, and survive a hot reload
with their spawn positions — a source edit to `pos` does not move an entity that already exists."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from osvauld.session import Session, shell_binary


def readout(tree, prefix):
    if isinstance(tree, dict):
        if str(tree.get("text", "")).startswith(prefix):
            return tree["text"][len(prefix):]
        tree = list(tree.values())
    if isinstance(tree, list):
        for child in tree:
            found = readout(child, prefix)
            if found is not None:
                return found
    return None


with Session(shell_binary=shell_binary(), offscreen=(900, 700)) as s:
    s.rpc.signup("world", "world passphrase")
    ws = s.rpc.create_workspace("world smoke")
    item = s.rpc.create_item(ws["id"], "world", "app")["id"]
    s.rpc.upload_folder(item, Path(__file__).parent.parent / "demo_apps" / "world")
    s.rpc.open_item(item)
    s.rpc.frame(2)
    assert not s.rpc.read_console(item), s.rpc.read_console(item)
    room = next(r for r in s.rpc.rects() if r["id"] == "room")

    def entity_at(x, y):
        s.rpc.move_to(room["x"] + x, room["y"] + y)
        return readout(s.rpc.dump_tree(item), "entity: ")

    # Hero spawned at (120, 80); its body is ~(80, 130) into the drawing.
    assert entity_at(200, 210) == "hero"
    assert entity_at(570, 300) == "chest"
    assert entity_at(400, 230) == "—", "friend is not described yet"

    s.rpc.click(item, "add")
    s.rpc.frame(1)
    assert entity_at(400, 230) == "friend"
    s.rpc.click(item, "remove")
    s.rpc.frame(1)
    assert entity_at(400, 230) == "—", "friend despawned"

    # Clips play in Rust on the virtual clock. Closed, the lid covers (568, 260); clicking the
    # chest plays `open` (0.5s), which swings the lid back off that point, and `close` returns it.
    assert entity_at(568, 260) == "chest"
    for step, wanted in [("open", "—"), ("close", "chest")]:
        s.rpc.click_at(room["x"] + 570, room["y"] + 300)
        s.rpc.frame(1)
        s.rpc.advance(0.6)
        s.rpc.frame(2)
        assert entity_at(568, 260) == wanted, f"after {step}"

    # Hot reload with the hero's spawn pos changed: the world survives, so the hero stays put.
    source = s.rpc.read_file_versioned(item, "main.lua")
    result = s.rpc.edit_file(item, "main.lua", source["revision"], [
        {"old_text": 'pos = { 120, 80 }, drawing = hero', "new_text": 'pos = { 400, 80 }, drawing = hero'},
    ])
    assert result["persisted"] and result["activation"] == "activated", result
    s.rpc.frame(2)
    assert not s.rpc.read_console(item), s.rpc.read_console(item)
    assert entity_at(200, 210) == "hero", "reload reset the world"

    # WASD through the real keyboard path: held D moves the hero right at 160/s in Rust. The body
    # starts under (200, 210); half a second of frames carries it ~80 right, and releasing stops it.
    s.rpc.keyboard("KeyD", "d", True)
    s.rpc.frame(30)
    s.rpc.keyboard("KeyD", "d", False)
    s.rpc.frame(2)
    assert not s.rpc.read_console(item), s.rpc.read_console(item)
    assert entity_at(200, 210) == "—", "the hero walked away"
    assert entity_at(285, 210) == "hero", "the hero walked ~80 right"
    s.rpc.frame(30)
    assert entity_at(285, 210) == "hero", "released, the hero stopped"

    # Facing: the world shows a view per direction. Hits name the entity, not the part, so each
    # view is told apart by its outline, measured by bisecting outward from a point inside it:
    # the profile's nose makes the head lopsided against the body's centre (mirrored for left),
    # and the back view's bun raises the top of the head.
    def reach(x, y, dx, dy):
        inside, outside = 0, 90
        while outside - inside > 1:
            mid = (inside + outside) // 2
            if entity_at(x + mid * dx, y + mid * dy) == "hero":
                inside = mid
            else:
                outside = mid
        return inside

    def outline():
        hx, head_y, body_y = 280, 80 + 61, 80 + 128  # the hero stands near (200, 80) by now
        body_mid = hx + (reach(hx, body_y, 1, 0) - reach(hx, body_y, -1, 0)) / 2
        lopsided = (hx + reach(hx, head_y, 1, 0) - body_mid) - (body_mid - hx + reach(hx, head_y, -1, 0))
        return lopsided, reach(hx, head_y, 0, -1)

    def tap(code, key):
        s.rpc.keyboard(code, key, True)
        s.rpc.frame(1)
        s.rpc.keyboard(code, key, False)
        s.rpc.frame(2)

    lopsided, top = outline()
    assert lopsided > 12 and top < 47, ("facing right: profile", lopsided, top)
    tap("KeyA", "a")
    lopsided, top = outline()
    assert lopsided < -12 and top < 47, ("facing left: mirrored profile", lopsided, top)
    tap("KeyW", "w")
    lopsided, top = outline()
    assert abs(lopsided) < 10 and top > 47, ("facing up: back view with its bun", lopsided, top)
    tap("KeyS", "s")
    lopsided, top = outline()
    assert abs(lopsided) < 10 and top < 47, ("facing down: front view", lopsided, top)

    if len(sys.argv) > 1:
        s.rpc.click(item, "add")
        s.rpc.frame(1)
        s.rpc.save_screenshot(item, sys.argv[1])
    print("world: spawn/despawn by id, hits by id, clips on the clock, survives reload, WASD, facing views")
