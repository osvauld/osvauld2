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

    # The chest's sensor: far from it, the hero is not near, and E does nothing.
    def near():
        return readout(s.rpc.dump_tree(item), "near the chest: ")

    assert near() == "no"
    s.rpc.keyboard("KeyE", "e", True)
    s.rpc.frame(1)
    s.rpc.keyboard("KeyE", "e", False)
    s.rpc.frame(2)
    assert entity_at(570, 300) == "chest", "E out of reach: the chest stays put"

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
        {"old_text": 'pos = { 120, 80 }, drawing = views', "new_text": 'pos = { 400, 80 }, drawing = views'},
    ])
    assert result["persisted"] and result["activation"] == "activated", result
    s.rpc.frame(2)
    assert not s.rpc.read_console(item), s.rpc.read_console(item)
    assert entity_at(200, 210) == "hero", "reload reset the world"

    # WASD through the real keyboard path: held D moves the hero right at 160/s in Rust. The body
    # starts under (200, 210); half a second of frames carries it ~80 right, and releasing stops it.
    # The world reports the change of direction to Lua (`on_move`), not every frame.
    s.rpc.keyboard("KeyD", "d", True)
    s.rpc.frame(30)
    assert readout(s.rpc.dump_tree(item), "heading: ") == "1,0", "on_move reported the start"
    s.rpc.keyboard("KeyD", "d", False)
    s.rpc.frame(2)
    assert not s.rpc.read_console(item), s.rpc.read_console(item)
    assert readout(s.rpc.dump_tree(item), "heading: ") == "still", "on_move reported the stop"
    assert entity_at(200, 210) == "—", "the hero walked away"
    assert entity_at(285, 210) == "hero", "the hero walked ~80 right"
    s.rpc.frame(30)
    assert entity_at(285, 210) == "hero", "released, the hero stopped"

    # Jump: Space is an action; Lua plays the once `jump` clip and clears it on `on_clip_end`.
    # Standing, (280, 100) is the back wall just above the head; at the top of the jump the head
    # covers it.
    assert entity_at(280, 100) == "wall:n", "above the head"
    s.rpc.keyboard("Space", " ", True)
    s.rpc.frame(1)
    s.rpc.keyboard("Space", " ", False)
    s.rpc.advance(0.2)
    s.rpc.frame(1)
    assert entity_at(280, 100) == "hero", "mid-jump, the head is up there"
    s.rpc.advance(0.4)
    s.rpc.frame(2)
    assert not s.rpc.read_console(item), s.rpc.read_console(item)
    assert entity_at(280, 100) == "wall:n", "landed"
    assert entity_at(285, 210) == "hero", "the feet never moved"
    # Only `on_clip_end` clearing `jumping` lets the clip handle change and a second jump replay.
    s.rpc.keyboard("Space", " ", True)
    s.rpc.frame(1)
    s.rpc.keyboard("Space", " ", False)
    s.rpc.advance(0.2)
    s.rpc.frame(1)
    assert entity_at(280, 100) == "hero", "a second jump, after on_clip_end"
    s.rpc.advance(0.4)
    s.rpc.frame(2)

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

    def hold(code, key, frames):
        s.rpc.keyboard(code, key, True)
        s.rpc.frame(frames)
        s.rpc.keyboard(code, key, False)
        s.rpc.frame(2)

    # The chest is solid by its footprint: walking straight at it, the hero's feet stop short.
    hold("KeyD", "d", 102)
    assert (entity_at(510, 280), entity_at(520, 280)) == ("hero", "—"), "stopped by the chest"

    # Draw order by feet (`order = "y"`): a little higher and further right, the hero's feet are
    # above the chest's, so the chest covers its legs; coming round below, the hero covers it.
    hold("KeyW", "w", 6)
    hold("KeyD", "d", 22)
    assert (entity_at(570, 240), entity_at(570, 250)) == ("hero", "chest"), "behind the chest"
    hold("KeyD", "d", 60)
    hold("KeyS", "s", 30)
    hold("KeyA", "a", 50)
    assert entity_at(550, 280) == "hero", "come round below: in front of the chest"
    tap("KeyS", "s")  # face down again, as the carry checks expect

    # Carrying: E is an action; Lua attaches the chest to the hero's body, and Rust keeps it there.
    # Held in front, the chest covers the hero's body; walking left (a flipped view) carries it
    # ~80 left, and after E again it stays where it was dropped while the hero walks off.
    assert entity_at(560, 300) == "hero", "standing in front of the chest"
    assert near() == "yes", "on_zone: the hero walked into the chest's zone"
    tap("KeyE", "e")
    assert entity_at(560, 300) == "chest", "picked up, held in front of the hero"
    assert near() == "no", "carried, the chest has no zone"
    # Carried, the chest is off the floor: it neither blocks the hero nor stops at its own
    # footprint's old place.
    hold("KeyA", "a", 30)
    assert (entity_at(480, 300), entity_at(560, 300)) == ("chest", "—"), "carried left"
    tap("KeyE", "e")
    assert near() == "yes", "put down beside the hero, its zone is back"
    s.rpc.keyboard("KeyD", "d", True)
    s.rpc.frame(60)
    s.rpc.keyboard("KeyD", "d", False)
    s.rpc.frame(2)
    assert not s.rpc.read_console(item), s.rpc.read_console(item)
    assert (entity_at(480, 300), entity_at(635, 250)) == ("chest", "hero"), "dropped, left behind"

    # Walls: the hero collides by its feet. Walking on into the east wall it stops with its body
    # just inside, the nose behind the wall; walking up it stops at the foot of the tall back
    # wall, the whole hero still in the room — the world clips anything that spills past it.
    s.rpc.keyboard("KeyD", "d", True)
    s.rpc.frame(120)
    s.rpc.keyboard("KeyD", "d", False)
    s.rpc.frame(2)
    assert (entity_at(700, 210), entity_at(714, 210)) == ("hero", "wall:e"), "stopped at the east wall"
    s.rpc.keyboard("KeyW", "w", True)
    s.rpc.frame(120)
    s.rpc.keyboard("KeyW", "w", False)
    s.rpc.frame(2)
    assert entity_at(680, 30) == "hero", "stopped at the back wall, head inside the room"
    assert entity_at(680, 196) == "hero", "feet at the foot of the back wall"

    if len(sys.argv) > 1:
        s.rpc.click(item, "add")
        s.rpc.frame(1)
        s.rpc.save_screenshot(item, sys.argv[1])
    print("world: spawn/despawn by id, hits by id, clips on the clock, survives reload, WASD, facing views, order by feet, on_action and on_move, jump, carry, walls, a solid chest, a sensor")
