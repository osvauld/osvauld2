"""2D characters on the real path (plan: docs/design/2d-characters.md §0). S-C1: a square part
with a `scale_y` step key of 0.5 is half as tall and as wide as before at the frame
`rpc.advance` lands on, by pixels and by the pointer; `drawing:pose` takes `scale_x` too.
S-C2: a part a `show` track hides from 0.20 s to 0.40 s is the background at 0.25 s and its own
colour at 0.15 s and 0.45 s, its child drawn throughout; a plain frame posed from `clip:at` agrees.
S-C3: one quadrant of a four-colour PNG, turned 90° and scaled ×3 in a drawing, is its own colour
edge to edge with none of its neighbours' at the place `rpc.rects()` reports, answers `on_hover`
over its rectangle, and the dump lists its path, `src` and bytes. S-C5: `gfx.image` on a plain
frame draws a prop PNG at its natural size. S-C4: `character_lab`'s Paka against the pack's 32
frames (silhouette IoU, colour distance; side by sides in shots/character_lab/). S-C6: one exact
edit to `clips/attack.lua` over the bridge moves the sword. S-C7: three vector robots on Paka's
skeleton play Paka's clip files beside it, the same part under the pointer at the same place."""

import base64
import struct
import sys
import zlib
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from osvauld.png import Image, near
from osvauld.session import Session, shell_binary

ROOT = Path(__file__).resolve().parent.parent
VIEWPORT = (900, 700)
SQUARE = "#ff00ff"
CHILD = "#00ffff"
STAGE_BG = "#202020"


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


def extent(img, k, box, along):
    """The square's span in `box` units, along the row (`"x"`) or column (`"y"`) through its middle."""
    cx, cy = box["x"] + box["w"] / 2, box["y"] + box["h"] / 2
    if along == "x":
        hits = [x for x in range(int(box["x"]), int(box["x"] + box["w"])) if near(img.at(x * k, cy * k), SQUARE, 40)]
    else:
        hits = [y for y in range(int(box["y"]), int(box["y"] + box["h"])) if near(img.at(cx * k, y * k), SQUARE, 40)]
    return (max(hits) - min(hits) + 1) if hits else 0


def png(w, h, px):
    """An 8-bit RGBA PNG of `px(x, y) -> (r, g, b, a)`, stdlib only."""
    raw = b"".join(b"\0" + bytes(c for x in range(w) for c in px(x, y)) for y in range(h))
    chunk = lambda kind, data: (struct.pack(">I", len(data)) + kind + data
                                + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF))
    header = struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b"")


QUADS = {(False, False): (255, 0, 0, 255), (True, False): (0, 255, 0, 255),
         (False, True): (0, 0, 255, 255), (True, True): (255, 255, 0, 255)}
CRATE = (0xB0, 0x70, 0x30, 255)


def find(tree, el_id):
    if isinstance(tree, dict):
        if tree.get("id") == el_id:
            return tree
        tree = list(tree.values())
    if isinstance(tree, list):
        for child in tree:
            found = find(child, el_id)
            if found is not None:
                return found
    return None


def picture_main():
    with Session(shell_binary=shell_binary(), offscreen=VIEWPORT) as s:
        rpc = s.rpc
        rpc.signup("rigger", "rigger passphrase")
        ws = rpc.create_workspace("characters")
        item = rpc.create_item(ws["id"], "clip picture", "app")["id"]
        rpc.upload_folder(item, ROOT / "demo_apps" / "clip_picture")
        rpc.write_binary_file(item, "pics/quads.png", png(64, 64, lambda x, y: QUADS[(x >= 32, y >= 32)]))
        rpc.write_binary_file(item, "pics/crate.png", png(40, 24, lambda x, y: CRATE))
        rpc.open_item(item)
        rpc.frame(2)
        assert not rpc.read_console(item), rpc.read_console(item)
        rects = {r["id"]: r for r in rpc.rects()}
        turned, prop = rects["turned"], rects["prop"]
        assert (turned["w"], turned["h"]) == (96, 96), turned
        img = Image(base64.b64decode(rpc.screenshot(item)["png_base64"]))
        k = img.width / VIEWPORT[0]

        # S-C3: every device pixel inside the box is green, or green over the background at a
        # fractional edge; red, yellow or blue anywhere means a neighbour bled in.
        x0, y0 = round(turned["x"] * k), round(turned["y"] * k)
        x1, y1 = round((turned["x"] + turned["w"]) * k), round((turned["y"] + turned["h"]) * k)
        worst = 0
        for y in range(y0, y1):
            for x in range(x0, x1):
                r, g, b = img.at(x, y)[:3]
                worst = max(worst, r - 0x20, b - 0x20)
                if x0 < x < x1 - 1 and y0 < y < y1 - 1:
                    assert near((r, g, b), "#00ff00", 6), ("inside", x, y, (r, g, b))
        assert worst <= 6, ("a neighbour's colour at the edge", worst)
        for x, y in [(x0 - 2, (y0 + y1) // 2), (x1 + 1, (y0 + y1) // 2), ((x0 + x1) // 2, y1 + 1)]:
            assert near(img.at(x, y), STAGE_BG, 12), ("outside", x, y, img.at(x, y))

        def part_at(x, y):
            rpc.move_to(x, y)
            return readout(rpc.dump_tree(item), "part: ")

        assert part_at(turned["x"] + 2, turned["y"] + turned["h"] - 2) == "tile"
        assert part_at(turned["x"] + 48, turned["y"] + 48) == "tile"
        assert part_at(turned["x"] + 98, turned["y"] + 48) == "—"
        pictures = find(rpc.dump_tree(item), "turned")["pictures"]
        assert pictures == [{"path": "pics/quads.png", "src": [32, 0, 32, 32], "bytes": 4096,
                             "shared": False}], pictures

        # S-C5: the prop at natural size, its whole picture shared with the texture.
        assert (prop["w"], prop["h"]) == (40, 24), prop
        row = (prop["y"] + 12) * k
        span = [x for x in range(round((prop["x"] - 4) * k), round((prop["x"] + 44) * k))
                if near(img.at(x, row), "#b07030", 8)]
        assert abs(len(span) / k - 40) <= 1, ("prop width", len(span) / k)
        props = find(rpc.dump_tree(item), "prop")["pictures"]
        assert props == [{"path": "pics/crate.png", "src": [0, 0, 40, 24], "bytes": 3840, "shared": True}], props
        assert not rpc.read_console(item), rpc.read_console(item)
    print(f"picture smoke ok (edge excess {worst}, prop {len(span) / k:.0f} px)")


def show_main():
    with Session(shell_binary=shell_binary(), offscreen=VIEWPORT) as s:
        rpc = s.rpc
        rpc.signup("rigger", "rigger passphrase")
        ws = rpc.create_workspace("characters")
        item = rpc.create_item(ws["id"], "clip show", "app")["id"]
        rpc.upload_folder(item, ROOT / "demo_apps" / "clip_show")
        rpc.open_item(item)
        rpc.frame(1)
        assert not rpc.read_console(item), rpc.read_console(item)
        rects = {r["id"]: r for r in rpc.rects()}
        stage, frozen = rects["stage"], rects["frozen"]

        def shot():
            img = Image(base64.b64decode(rpc.screenshot(item)["png_base64"]))
            return img, img.width / VIEWPORT[0]

        def colours(box):
            img, k = shot()
            at = lambda fx: img.at((box["x"] + fx * box["w"]) * k, (box["y"] + box["h"] / 2) * k)
            return at(0.25), at(0.75)  # the square's middle, the child's middle

        # Square at 0.25 of the stage's width, its child at 0.75; the world's own clock moves
        # only on `rpc.advance`.
        rpc.advance(0.15)
        before = colours(stage)
        assert near(before[0], SQUARE, 40) and near(before[1], CHILD, 40), ("0.15 s", before)
        rpc.advance(0.10)
        hidden = colours(stage)
        assert near(hidden[0], STAGE_BG, 12), ("hidden at 0.25 s", hidden)
        assert near(hidden[1], CHILD, 40), ("its child still drawn", hidden)
        rpc.advance(0.20)
        after = colours(stage)
        assert near(after[0], SQUARE, 40) and near(after[1], CHILD, 40), ("0.45 s", after)

        # A plain frame posed from `clip:at(0.3)` draws the same hidden pose.
        posed = colours(frozen)
        assert not near(posed[0], SQUARE, 40) and near(posed[1], CHILD, 40), ("clip:at(0.3)", posed)
        assert not rpc.read_console(item), rpc.read_console(item)
    print(f"show smoke ok (square {before[0]} -> {hidden[0]} -> {after[0]})")


LAB = ROOT / "demo_apps" / "character_lab"
PAKA = LAB / "characters" / "paka"
LAB_VIEWPORT = (1200, 800)
SHOTS = ROOT / "shots" / "character_lab"
# From the first real run (2026-11-10): IoU 0.971-0.991, colour 3.4-9.2; the worst frames are
# where the rigid-drawn hair crest and ears stray (attack 15-20).
MIN_IOU, MAX_COLOUR = 0.96, 12.0


def open_lab(rpc):
    rpc.signup("rigger", "rigger passphrase")
    ws = rpc.create_workspace("characters")
    item = rpc.create_item(ws["id"], "character lab", "app")["id"]
    rpc.upload_folder(item, LAB)
    rpc.open_item(item)
    rpc.frame(2)
    assert not rpc.read_console(item), rpc.read_console(item)
    return item


def lab_shot(rpc, item):
    img = Image(base64.b64decode(rpc.screenshot(item, scale=1.0)["png_base64"]))
    assert img.width == LAB_VIEWPORT[0], ("a logical pixel per pixel", img.width)
    return img


def pose_at(rpc, item, anim, t):
    """Paused on `anim` at exactly `t`: the lab's frame is posed from `clip:at(t)`."""
    rpc.click(item, f"clip:{anim}")
    rpc.type_text(item, "time", repr(t))
    tree = rpc.dump_tree(item)
    assert readout(tree, "clip: ") == anim, readout(tree, "clip: ")
    assert readout(tree, "time: ").startswith(f"{t:.4f} "), (readout(tree, "time: "), t)


def answer_key_main():
    """S-C4: Paka drawn by the shell against the pack's own frames, 8 times each of 4 clips."""
    import json
    import preview_character as P

    frames = json.loads((P.FRAMES / "frames.json").read_text())["frames"]
    SHOTS.mkdir(parents=True, exist_ok=True)
    rows, offsets = [], {}
    with Session(shell_binary=shell_binary(), offscreen=LAB_VIEWPORT) as s:
        rpc = s.rpc
        item = open_lab(rpc)
        for anim in ("run", "attack", "jump", "idle"):
            for f in sorted((f for f in frames if f["anim"] == anim), key=lambda f: f["frame"]):
                pose_at(rpc, item, anim, f["frame"] / 60)
                char = {r["id"]: r for r in rpc.rects()}["char:paka"]
                # The drawing's box, unclipped, its top-left the drawing's origin (the slot's
                # column stretches its width).
                assert char["w"] >= 396 and char["h"] == 239, char
                img = lab_shot(rpc, item)
                pack = Image((P.FRAMES / f["file"]).read_bytes())
                origin = (round(char["x"]), round(char["y"]))
                if anim not in offsets:  # one alignment per animation's canvas, held for its 8
                    offsets[anim] = P.fit(pack, img, origin)
                iou, colour, box = P.score(pack, img, origin, offsets[anim])
                P.side_by_side(pack, img, box, SHOTS / f"{anim}_f{f['frame']:03d}.png")
                rows.append((anim, f["frame"], offsets[anim], iou, colour))
        assert not rpc.read_console(item), rpc.read_console(item)
    print(f"{'anim':7} {'frame':>5} {'offset':>10} {'IoU':>6} {'colour':>7}")
    for anim, frame, d, iou, colour in rows:
        print(f"{anim:7} {frame:5d} {str(d):>10} {iou:6.3f} {colour:7.1f}")
    bad = [r for r in rows if r[3] < MIN_IOU or r[4] > MAX_COLOUR]
    assert not bad, ("past the thresholds", MIN_IOU, MAX_COLOUR, bad)
    print(f"answer key ok: IoU >= {min(r[3] for r in rows):.3f}, colour <= {max(r[4] for r in rows):.1f}"
          f" (side by sides in {SHOTS})")


def sword_point(rig, clip_text, t):
    """A point on the in-hand sword, in drawing units, from the converter test's evaluator of the
    rig and this text of the clip."""
    import tempfile
    import test_spine_to_lua as T

    with tempfile.NamedTemporaryFile("w", suffix=".lua") as f:
        f.write(clip_text)
        f.flush()
        clip = T.lua_load(f.name)
    part = next(p for p in rig["parts"] if p["id"] == "slot:sword_forward")
    sh = part["shapes"][0]
    w, h = sh["src"][2], sh["src"][3]
    corners = [T.apply(sh["transform"], c) for c in ((0, 0), (w, 0), (0, h), (w, h))]
    centre = T.apply(sh["transform"], (w / 2, h / 2))
    tip = max(corners, key=lambda c: (c[0] - part["pivot"][0]) ** 2 + (c[1] - part["pivot"][1]) ** 2)
    # A fifth of the way from the picture's centre to its far corner: on the blade, inside the
    # frame's box (only that is hovered) before and after the edit, with nothing drawn over it.
    rest = (centre[0] + 0.2 * (tip[0] - centre[0]), centre[1] + 0.2 * (tip[1] - centre[1]))
    return T.apply(T.worlds(rig, T.sample(clip, t))["slot:sword_forward"], rest)


def hot_edit_main():
    """S-C6: one exact edit to `clips/attack.lua` over the bridge; the app reloads on the same
    clip and time and the sword is elsewhere."""
    import test_spine_to_lua as T

    # sword_forward's rot key at 0.3333 s, turned 90°; 8/60 s is on its way to that key.
    path, t = "characters/paka/clips/attack.lua", 8 / 60
    old, new = "{0.3333, -67.13, ", "{0.3333, 22.87, "
    rig = T.lua_load(PAKA / "rig.lua")
    with Session(shell_binary=shell_binary(), offscreen=LAB_VIEWPORT) as s:
        rpc = s.rpc
        item = open_lab(rpc)
        pose_at(rpc, item, "attack", t)
        char = {r["id"]: r for r in rpc.rects()}["char:paka"]

        def at(p):
            x, y = char["x"] + p[0], char["y"] + p[1]
            rpc.move_to(x, y)
            return readout(rpc.dump_tree(item), "part: "), lab_shot(rpc, item).at(x, y)

        src = rpc.read_file_versioned(item, path)
        assert src["content"].count(old) == 1, "the key, once"
        was = sword_point(rig, src["content"], t)
        will = sword_point(rig, src["content"].replace(old, new), t)
        assert (was[0] - will[0]) ** 2 + (was[1] - will[1]) ** 2 > 30 ** 2, (was, will)
        part, colour = at(was)
        assert part == "slot:sword_forward", ("the sword before", part)
        assert not near(colour, "#00ffff", 40), ("the sword's pixels before", colour)

        result = rpc.edit_file(item, path, src["revision"], [{"old_text": old, "new_text": new}])
        assert result["persisted"] and result["activation"] == "activated", result
        rpc.frame(2)
        assert not rpc.read_console(item), rpc.read_console(item)
        tree = rpc.dump_tree(item)
        assert readout(tree, "clip: ") == "attack" and readout(tree, "time: ").startswith(f"{t:.4f} "), \
            ("the reload kept the clip and time", readout(tree, "clip: "), readout(tree, "time: "))
        gone, colour_after = at(was)
        assert gone != "slot:sword_forward" and near(colour_after, "#00ffff", 40), ("the sword gone", gone, colour_after)
        moved, there = at(will)
        assert moved == "slot:sword_forward" and not near(there, "#00ffff", 40), ("the sword moved here", moved, there)
    print(f"hot edit ok (sword {was[0]:.0f},{was[1]:.0f} -> {will[0]:.0f},{will[1]:.0f}; "
          f"{colour} -> {colour_after} at the old place)")


ROBOTS = ("scout", "brute", "volt")
# (clip, time, part, a point on both characters' piece for that part, in the setup pose). The
# robots' pieces are drawn in the setup pose (characters/robot/build.lua), so these are points
# of their art; each is inside Paka's picture for the part too, with nothing of either drawn over.
PROBES = [
    ("run", 0.1, "slot:head_main", (205, 66)),
    ("run", 0.1, "slot:body", (180, 195)),
    ("run", 0.1, "slot:hoof_front", (172, 237)),
    ("run", 0.4, "slot:head_main", (205, 66)),
    ("run", 0.4, "slot:hoof_front", (172, 237)),
    ("attack", 8 / 60, "slot:sword_forward", (337, 160)),
    ("attack", 8 / 60, "slot:head_main", (205, 66)),
]


def our_own_main():
    """S-C7: three vector robots on Paka's skeleton play Paka's clip files beside Paka; the same
    part is in the same place in both, by the pointer; they are drawn and they move."""
    import test_spine_to_lua as T

    for path in (LAB / "characters" / "robot").rglob("*.lua"):
        assert "image" not in path.read_text(), ("a picture in the robot", path.name)
    # Their own clips (brawl-movement.md step 5) are authored on the same skeleton and listed in cast.lua.
    cast = (LAB / "cast.lua").read_text()
    for path in (LAB / "characters" / "robot" / "clips").glob("*.lua"):
        assert f'"{path.stem}"' in cast, ("a robot clip the picker doesn't list", path.name)
    rig = T.lua_load(PAKA / "rig.lua")
    setup = T.worlds(rig, T.sample(T.lua_load(PAKA / "setup.lua"), 0))
    clips = {name: T.lua_load(PAKA / "clips" / f"{name}.lua") for name in ("run", "attack")}

    def inverse(m):
        a, b, c, d, e, f = m
        det = a * d - b * c
        return (d / det, -b / det, -c / det, a / det, (c * f - d * e) / det, (b * e - a * f) / det)

    def drawn_at(anim, t, part, p):
        rest = T.apply(inverse(setup[part]), p)
        return T.apply(T.worlds(rig, T.sample(clips[anim], t))[part], rest)

    with Session(shell_binary=shell_binary(), offscreen=LAB_VIEWPORT) as s:
        rpc = s.rpc
        item = open_lab(rpc)
        tree = rpc.dump_tree(item)
        assert find(tree, "char:paka")["pictures"], "Paka is pictures"
        for robot in ROBOTS:
            assert not find(tree, f"char:{robot}").get("pictures"), (robot, "drew a picture")

        def part_at(x, y):
            rpc.move_to(x, y)
            return readout(rpc.dump_tree(item), "part: ")

        seen = []
        for anim, t, part, p in PROBES:
            pose_at(rpc, item, anim, t)
            rects = {r["id"]: r for r in rpc.rects()}
            d = drawn_at(anim, t, part, p)
            img = lab_shot(rpc, item)
            checked = 0
            for who in ("paka",) + ROBOTS:
                box = rects[f"char:{who}"]
                x, y = box["x"] + d[0], box["y"] + d[1]
                if x >= box["x"] + box["w"]:  # past the stage's edge, where the stage clips it
                    continue
                checked += 1
                got = part_at(x, y)
                assert got == part, (anim, t, who, part, "is", got, "at", d)
                if who != "paka":
                    assert not near(img.at(x, y), "#00ffff", 40), (anim, t, who, part, "not drawn", img.at(x, y))
            assert checked >= 3, (anim, t, part, "seen on", checked)
            seen.append((anim, round(t, 3), part, checked))

        # It moves: at 0.4 s of `run` the front boot is 60 units back from where it was at 0.1 s.
        boot = drawn_at("run", 0.1, "slot:hoof_front", (172, 237))
        pose_at(rpc, item, "run", 0.4)
        rects = {r["id"]: r for r in rpc.rects()}
        img = lab_shot(rpc, item)
        for robot in ROBOTS:
            box = rects[f"char:{robot}"]
            x, y = box["x"] + boot[0], box["y"] + boot[1]
            assert part_at(x, y) != "slot:hoof_front", (robot, "the boot stayed")

        # One world, every clip track finding its part (a missing one is a console line).
        rpc.click(item, "clip:run")
        rpc.click(item, "play")
        rpc.frame(30)
        entities = {e["id"]: e for e in find(rpc.dump_tree(item), "world")["world"]["entities"]}
        assert set(entities) == {"paka", *ROBOTS}, entities.keys()
        clips_seen = {(e["clip"]["time"], e["clip"]["length"]) for e in entities.values()}
        assert len(clips_seen) == 1, ("one clip, one clock", clips_seen)
        assert not rpc.read_console(item), rpc.read_console(item)
        rpc.click(item, "play")
    print(f"our own ok ({len(ROBOTS)} robots, vector only; same part, same place: {seen})")


def main():
    with Session(shell_binary=shell_binary(), offscreen=VIEWPORT) as s:
        rpc = s.rpc
        rpc.signup("rigger", "rigger passphrase")
        ws = rpc.create_workspace("characters")
        item = rpc.create_item(ws["id"], "clip scale", "app")["id"]
        rpc.upload_folder(item, ROOT / "demo_apps" / "clip_scale")
        rpc.open_item(item)
        rpc.frame(2)
        assert not rpc.read_console(item), rpc.read_console(item)
        rects = {r["id"]: r for r in rpc.rects()}
        stage, narrow = rects["stage"], rects["narrow"]
        assert (stage["w"], stage["h"]) == (400, 300), stage
        # The row stretches the leaf to the stage's height; its drawing is the top 200.
        narrow = dict(narrow, h=200)

        def shot():
            img = Image(base64.b64decode(rpc.screenshot(item)["png_base64"]))
            return img, img.width / VIEWPORT[0]

        def entity_at(x, y):
            rpc.move_to(stage["x"] + x, stage["y"] + y)
            return readout(rpc.dump_tree(item), "entity: ")

        # The entity's box is (100, 50)..(300, 250) in the stage; the square its middle 100.
        rpc.advance(0.3)
        img, k = shot()
        before = (extent(img, k, stage, "x"), extent(img, k, stage, "y"))
        assert all(abs(v - 100) <= 2 for v in before), ("before the step key", before)
        assert entity_at(200, 110) == "box", "the square's top band is drawn"

        rpc.advance(0.3)  # past the step key at 0.5 s
        img, k = shot()
        after = (extent(img, k, stage, "x"), extent(img, k, stage, "y"))
        assert abs(after[0] - before[0]) <= 1 and abs(after[1] - before[1] / 2) <= 2, ("after", before, after)
        assert entity_at(200, 110) == "—", "squashed about its pivot: the top band is empty"
        assert entity_at(200, 140) == "box"

        # A plain frame posed with scale_x 0.5: half as wide, as tall.
        posed = (extent(img, k, narrow, "x"), extent(img, k, narrow, "y"))
        assert abs(posed[0] - 50) <= 2 and abs(posed[1] - 100) <= 2, ("pose scale_x", posed)
        assert not rpc.read_console(item), rpc.read_console(item)
    print(f"characters smoke ok (square {before} -> {after}; posed {posed})")


if __name__ == "__main__":
    main()
    show_main()
    picture_main()
    answer_key_main()
    hot_edit_main()
    our_own_main()
