"""Jetpack editors over the real path (design/jetpack-editors.md): a map and a pilot made in their
editors by real pointer hits are what the game plays; with no asset the game plays its baked ones."""
import base64
import shutil
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from osvauld.session import Session, shell_binary
from osvauld.png import Image, near
from bake_jetpack import bake, lua

ROOT = Path(__file__).resolve().parent.parent
ASSET = "asset:jetpack-maps"
PILOTS = "asset:jetpack-pilots"
MAGENTA = "#ff00ff"


def find(tree, identity):
    if tree.get("id") == identity:
        return tree
    for child in tree.get("children", []):
        found = find(child, identity)
        if found:
            return found


def by_id(items):
    return {e["id"]: e for e in items}


def box(shape):
    xs = [c[k] for c in shape["path"] for k in range(1, len(c), 2)]
    ys = [c[k] for c in shape["path"] for k in range(2, len(c), 2)]
    return min(xs), min(ys), max(xs) - min(xs), max(ys) - min(ys)


with Session(shell_binary=shell_binary(), offscreen=(1200, 900)) as s:
    r = s.rpc
    r.signup("maker", "jetpack editors passphrase")
    ws = r.create_workspace("Jetpack editors")

    def app(name, folder, root=ROOT):
        item = r.create_item(ws["id"], name, "app")["id"]
        r.upload_folder(item, Path(root) / "demo_apps" / folder)
        r.open_item(item)
        r.frame(3)
        assert not r.read_console(item), r.read_console(item)
        return item

    def rect(identity):
        found = [e for e in r.rects() if e["id"] == identity]
        assert found, (identity, sorted(e["id"] for e in r.rects()))
        return found[0]

    def tap(identity):
        r.click_at(*r.centre_of(identity))
        r.frame(2)

    def label(item, identity):
        node = find(r.dump_tree(item), identity)
        assert node, (identity, r.read_console(item))
        return node["text"]

    # Test 1: the editor seeds the asset from its baked map, then edits it by pointer.
    editor = app("Map editor", "jetpack_map_editor")

    def yard():
        maps = r.read_data(editor)[ASSET]["maps"]
        return next(m for m in maps if m["id"] == "yard")

    seeded = yard()
    assert (seeded["w"], seeded["h"]) == (2880, 480), seeded
    baked_ids = {p["id"] for p in seeded["platforms"]}
    assert "p1" in baked_ids and len(seeded["spawns"]) == 3

    canvas = rect("map:canvas")  # zoom 1 at open: a map unit is a point, origin at its top-left

    def screen(x, y):
        return canvas["x"] + x, canvas["y"] + y

    tap("tool:ledge")
    r.click_at(*screen(300, 100))
    r.frame(2)
    added = [p for p in yard()["platforms"] if p["id"] not in baked_ids]
    assert len(added) == 1, added
    new = added[0]
    assert (new["kind"], new["x"], new["y"], new["w"], new["h"]) == ("ledge", 300, 100, 160, 18), new

    # Drag by its middle: one write on release, snapped to 10.
    r.drag(screen(380, 109), screen(431, 149))
    r.frame(2)
    moved = by_id(yard()["platforms"])[new["id"]]
    assert (moved["x"], moved["y"]) == (350, 140), moved
    # Drag its resize grip (bottom-right corner) 40 wider.
    r.drag(screen(508, 156), screen(548, 156))
    r.frame(2)
    resized = by_id(yard()["platforms"])[new["id"]]
    assert (resized["x"], resized["w"], resized["h"]) == (350, 200, 18), resized

    # The player spawn moves to a click; a bot spawn is added.
    tap("tool:player")
    r.click_at(*screen(420, 80))
    r.frame(2)
    you = [sp for sp in yard()["spawns"] if sp["team"] == "player"]
    assert [(sp["x"], sp["y"]) for sp in you] == [(420, 80)], you
    tap("tool:bot")
    assert canvas["w"] > 720, canvas  # the click below must land on the visible canvas
    r.click_at(*screen(700, 60))
    r.frame(2)
    assert sum(sp["team"] == "bot" for sp in yard()["spawns"]) == 3

    # Select p1 by clicking it, delete it.
    tap("tool:select")
    p1 = by_id(seeded["platforms"])["p1"]
    r.click_at(*screen(p1["x"] + p1["w"] / 2, p1["y"] + p1["h"] / 2))
    r.frame(2)
    tap("selected:delete")
    assert "p1" not in by_id(yard()["platforms"])

    # Map height through its field.
    r.type_text(editor, "map:h", "720")
    r.key(editor, "map:h", "enter")
    r.frame(2)
    edited = yard()
    assert edited["h"] == 720, edited["h"]
    # Shrinking past platforms is refused and says why; the map is unchanged.
    r.type_text(editor, "map:w", "2000")
    r.key(editor, "map:w", "enter")
    r.frame(2)
    assert yard() == edited, "a refused shrink changed the map"
    assert label(editor, "editor:status").startswith("Move or delete"), label(editor, "editor:status")
    r.type_text(editor, "map:w", "2880")
    r.key(editor, "map:w", "enter")
    r.frame(2)
    assert label(editor, "editor:status").endswith("ready for the game"), label(editor, "editor:status")
    assert not r.read_console(editor), r.read_console(editor)
    r.save_screenshot(editor, str(ROOT / "shots/jetpack-map-editor.png"))

    # Reopened from the vault, the editor shows the same map.
    r.request("CloseItem", item_id=editor)
    r.frame(2)
    r.open_item(editor)
    r.frame(3)
    assert yard() == edited, "the edited map did not survive closing the editor"

    # Test 3: the pilot editor seeds the scout, then edits it by pointer and by its fields.
    pilot_editor = app("Pilot editor", "jetpack_pilot_editor")

    def pilots():
        return r.read_data(pilot_editor)[PILOTS]["pilots"]

    def scout():
        return next(p for p in pilots() if p["id"] == "scout")

    def shapes(pilot):
        return {sh["id"]: sh for part in pilot["parts"] for sh in part["shapes"]}

    assert [p["id"] for p in pilots()] == ["scout"]
    seeded_scout = scout()
    assert seeded_scout["scale"] == 0.5 and seeded_scout["size"] == [104, 104], seeded_scout["size"]
    assert r.read_data(pilot_editor)[PILOTS]["info"]["version"] == 2
    pc = rect("pilot:canvas")
    zoom = pc["h"] / seeded_scout["size"][1]  # the element stretches across; the drawing does not
    assert zoom == 6, pc  # a pilot unit is six points, origin at the drawing's top-left

    def at(x, y):
        return pc["x"] + x * zoom, pc["y"] + y * zoom

    # One var recolours the visor everywhere it is named.
    r.type_text(pilot_editor, "var:visor", MAGENTA)
    r.key(pilot_editor, "var:visor", "enter")
    r.frame(2)
    assert scout()["vars"]["visor"] == MAGENTA, scout()["vars"]

    # The visor glass (a head shape) is picked by a click and dragged 3 units right.
    head = next(part for part in scout()["parts"] if part["id"] == "head")
    visor = next(sh for sh in head["shapes"] if sh["fill"] == "@visor")
    vx, vy, vw, vh = box(visor)
    r.click_at(*at(vx + vw - 3, vy + vh - 3))
    r.frame(2)
    assert label(pilot_editor, "shape:selected") == f"{visor['id']} · head", label(pilot_editor, "shape:selected")
    r.drag(at(vx + vw - 3, vy + vh - 3), at(vx + vw, vy + vh - 3))
    r.frame(2)
    assert box(shapes(scout())[visor["id"]]) == (vx + 3, vy, vw, vh), box(shapes(scout())[visor["id"]])

    # A rectangle added to the torso where the click lands, its box grip dragged 4 wider, then
    # filled with the visor var.
    before = set(shapes(scout()))
    tap("part:torso")
    tap("tool:rect")
    r.click_at(*at(60, 10))
    r.frame(2)
    added = [sh for i, sh in shapes(scout()).items() if i not in before]
    assert len(added) == 1 and box(added[0]) == (60, 10, 12, 12), added
    torso = next(part for part in scout()["parts"] if part["id"] == "torso")
    assert torso["shapes"][-1]["id"] == added[0]["id"], "a new shape draws on top of its part"
    new_shape = added[0]["id"]
    tap("tool:select")
    r.drag(at(72, 22), at(76, 22))
    r.frame(2)
    assert box(shapes(scout())[new_shape]) == (60, 10, 16, 12), box(shapes(scout())[new_shape])
    tap("fill:visor")
    assert shapes(scout())[new_shape]["fill"] == "@visor"

    # The palms and the hold move with their markers.
    pts = seeded_scout["points"]
    assert pts["palm_gun"] == [81, 59] and pts["palm_free"] == [94, 57] and pts["hold"] == [66, 60], pts
    assert sorted(pts) == ["eye", "hold", "jet", "palm_free", "palm_gun"], pts
    r.drag(at(81, 59), at(79, 60))
    r.frame(2)
    r.drag(at(94, 57), at(95, 58))
    r.frame(2)
    r.drag(at(66, 60), at(67, 61))
    r.frame(2)
    got = scout()["points"]
    assert got["palm_gun"] == [79, 60] and got["palm_free"] == [95, 58] and got["hold"] == [67, 61], got

    # Parts are a vertical list with the rig's ids, in draw order, each with its parent.
    PARTS = ["arm_free", "forearm_free", "hand_free", "leg_back", "leg_front", "jetpack", "torso", "head",
             "arm_gun", "forearm_gun", "hand_gun", "finger_gun", "thumb_gun"]
    assert [part["id"] for part in scout()["parts"]] == PARTS
    assert {part["id"]: part.get("parent") for part in scout()["parts"]}["finger_gun"] == "hand_gun"
    ys = [rect("part:" + p)["y"] for p in PARTS]
    assert ys == sorted(ys) and len(set(ys)) == 13, "the part list is vertical, in draw order"
    # Every part can be chosen and drawn into: a rectangle lands in it, on top.
    for part_id in ("forearm_free", "finger_gun", "hand_gun"):
        tap("part:" + part_id)
        tap("tool:rect")
        r.click_at(*at(34, 40))
        r.frame(2)
        arm = next(part for part in scout()["parts"] if part["id"] == part_id)
        assert box(arm["shapes"][-1]) == (34, 40, 12, 12), (part_id, arm["shapes"][-1])
        tap("shape:delete")
        tap("tool:select")
    tap("part:arm_gun")
    tap("tool:rect")
    r.click_at(*at(34, 40))
    r.frame(2)
    arm_gun = next(part for part in scout()["parts"] if part["id"] == "arm_gun")
    assert box(arm_gun["shapes"][-1]) == (34, 40, 12, 12), arm_gun["shapes"][-1]
    tap("shape:delete")
    tap("tool:select")

    # The jetpack sits behind the torso; picking its part brings it on top of the canvas.
    def pixel(x, y):
        image = Image(base64.b64decode(r.screenshot(pilot_editor)["png_base64"]))
        return image.at(*(int(v) for v in at(x, y)))

    DARK = "#709a78"  # the jetpack strap's colour, where the torso covers it
    tap("part:torso")
    assert not near(pixel(28, 60), DARK, 20), "the torso covers the jetpack strap"
    tap("part:jetpack")
    assert near(pixel(28, 60), DARK, 20), pixel(20, 60)
    tap("part:torso")

    # A second pilot starts as a copy of the current one.
    tap("pilot:new")
    assert len(pilots()) == 2 and pilots()[1]["id"] != "scout", [p["id"] for p in pilots()]
    assert label(pilot_editor, "editor:status").endswith("ready for the game"), label(pilot_editor, "editor:status")
    assert not r.read_console(pilot_editor), r.read_console(pilot_editor)
    r.save_screenshot(pilot_editor, str(ROOT / "shots/jetpack-pilot-editor.png"))
    edited_pilots = pilots()
    r.request("CloseItem", item_id=pilot_editor)
    r.frame(2)
    r.open_item(pilot_editor)
    r.frame(3)
    assert pilots() == edited_pilots, "the edited pilots did not survive closing the editor"

    # The preview plays the game's own drawing: flying shows the flame, and it moves.
    def shot(item):
        return Image(base64.b64decode(r.screenshot(item)["png_base64"]))

    def count(image, colour, box):
        x0, y0, w, h = int(box["x"]), int(box["y"]), int(box["w"]), int(box["h"])
        return sum(near(image.at(x, y), colour, 30) for y in range(y0, y0 + h) for x in range(x0, x0 + w))

    tap("motion:fly")
    # The previews are paint, not hit targets: they sit in the side panel under the motion row.
    walk, swatch = rect("motion:walk"), rect("fill:main")
    top = walk["y"] + walk["h"]
    preview = {"x": swatch["x"], "y": top, "w": 260, "h": 330}
    first = shot(pilot_editor)
    assert count(first, "#fbd484", preview) > 10, "no flame in the flying preview"
    r.frame(9)
    later = shot(pilot_editor)
    rows = range(int(preview["y"]), int(preview["y"] + preview["h"]))
    assert any(first.rows[y] != later.rows[y] for y in rows), "the preview did not move"
    tap("motion:play")  # paused: frames no longer change it
    paused = shot(pilot_editor)
    r.frame(9)
    still = shot(pilot_editor)
    assert all(paused.rows[y] == still.rows[y] for y in rows), "a paused preview moved"
    tap("motion:play")

    # The gun hand really holds the gun: glove pixels sit on the grip (the hold point at aim 0 or
    # its mirror) in the level preview, which starts at its first sky-coloured row.
    GLOVE = "#e6bf80"
    swatch_x = int(rect("fill:main")["x"])

    P_SKY = "#d7ded0"
    tap("motion:idle")
    image = shot(pilot_editor)
    hold = scout()["points"]["hold"]
    origin_y = next(y for y in range(int(top), int(top) + 400) if near(image.at(swatch_x + 120, y), P_SKY, 6))
    # The actor sits at (80, 30) in the 2x preview, so a drawing unit (half a game unit) is one pixel.
    gx_right, gy = swatch_x + 80 + hold[0], origin_y + 30 + hold[1]
    gx_left = swatch_x + 80 + (seeded_scout["size"][0] - hold[0])
    def glove_near(image, cx, cy, reach=8):
        return sum(near(image.at(x, y), GLOVE, 30) for y in range(int(cy) - reach, int(cy) + reach)
                   for x in range(int(cx) - reach, int(cx) + reach))
    held = max(glove_near(image, gx_right, gy), glove_near(image, gx_left, gy))
    assert held > 60, f"no glove on the grip: {held}"
    # Aiming sweeps the gun and squeezes the trigger: the glove stays with it, the preview changes.
    tap("motion:aim")
    r.frame(7)
    aiming = shot(pilot_editor)
    assert any(aiming.rows[y] != image.rows[y] for y in range(int(top), int(top) + 330)), "aim did not move the pilot"

    def magenta(item):
        image = Image(base64.b64decode(r.screenshot(item)["png_base64"]))
        return sum(near(image.at(x, y), MAGENTA, 40)
                   for y in range(0, image.height, 2) for x in range(0, image.width, 2))

    # Test 2: the game plays the edited map — the player spawns at (420, 80) and lands on the
    # moved ledge (top 140): feet 580 above the bottom of the 720-high map.
    game = app("Jetpack Arena", "jetpack")
    tap("map:pick:yard")  # the game opens on the first (big) arena; the editor edits the yard
    assert label(game, "map:name") == "Training Yard"
    tap("match:start")
    r.frame(90)
    assert label(game, "hud:altitude") == "ALT 580", label(game, "hud:altitude")
    # Test 3, played: every pilot wears the magenta visor and the gun-mounted plate.
    assert magenta(game) > 20, magenta(game)
    assert not r.read_console(game), r.read_console(game)
    r.save_screenshot(game, str(ROOT / "shots/jetpack-edited-map.png"))

    # Test 5: bake both editors' assets into a copy of the game's source.
    baked_root = Path(tempfile.mkdtemp(prefix="jetpack-bake-"))
    shutil.copytree(ROOT / "demo_apps" / "jetpack", baked_root / "demo_apps" / "jetpack")
    written = bake(r, [editor, pilot_editor], baked_root)
    assert baked_root / "demo_apps/jetpack/baked/maps.lua" in written, written
    assert baked_root / "demo_apps/jetpack/baked/pilots.lua" in written, written
    assert bake(r, [editor, pilot_editor], baked_root) == written and len(written) == 5, "re-bake must be stable"

    # Test 6 (missing asset): a workspace with no editor plays the baked yard — feet on the
    # ground, 30 above the bottom of the 480-high map.
    ws = r.create_workspace("Jetpack baked")
    r.select_workspace(ws["id"])
    plain = app("Jetpack Arena", "jetpack")
    tap("map:pick:yard")
    tap("match:start")
    r.frame(90)
    assert label(plain, "hud:altitude") == "ALT 30", label(plain, "hud:altitude")
    assert magenta(plain) == 0, "the repo's baked scout has a cyan visor"
    assert not r.read_console(plain), r.read_console(plain)

    # Test 5, played: with no asset either, the baked copy plays the edited map.
    shipped = app("Jetpack shipped", "jetpack", baked_root)
    tap("map:pick:yard")
    assert label(shipped, "map:name") == "Training Yard"
    tap("match:start")
    r.frame(90)
    assert label(shipped, "hud:altitude") == "ALT 580", label(shipped, "hud:altitude")
    assert magenta(shipped) > 20, "the baked copy plays the edited pilot"
    assert not r.read_console(shipped), r.read_console(shipped)
    shutil.rmtree(baked_root)

    # Upgrade: a workspace whose pilot asset is still version 1 (an armed scout and an armless one) is
    # upgraded in place when the pilot editor opens, shapes kept, and the game flies it.
    def shape(i, x, y, w, h, fill):
        return {"id": f"v{i}", "fill": fill, "path": [["move", x, y], ["line", x + w, y], ["line", x + w, y + h],
                                                      ["line", x, y + h], ["close"]]}

    v1 = {"id": "scout", "name": "Old scout", "size": [40, 52],
          "vars": {"main": "#d5e3a8", "dark": "#709a78", "ink": "#182e34", "visor": "#7fdddc"},
          "points": {"grip": [30, 27], "hand": [34, 24], "jet": [8, 35]},
          "parts": [{"id": "arm_free", "pivot": [23, 24], "shapes": [shape(1, 21, 21, 12, 6, "@ink")]},
                    {"id": "leg_back", "pivot": [16, 38], "shapes": [shape(2, 13, 37, 7, 11, "@ink")]},
                    {"id": "leg_front", "pivot": [25, 38], "shapes": [shape(3, 22, 37, 7, 11, "@ink")]},
                    {"id": "body", "pivot": [20, 30], "shapes": [shape(4, 12, 22, 16, 17, "@main"), shape(5, 17, 11, 12, 6, "@visor")]},
                    {"id": "arm_gun", "pivot": [19, 27], "shapes": [shape(6, 17, 24, 12, 6, "@dark")]}]}
    bare = {"id": "bare", "name": "Armless", "size": [40, 52], "vars": v1["vars"],
            "points": {"grip": [30, 27], "jet": [8, 35]},
            "parts": [v1["parts"][1], v1["parts"][2], v1["parts"][3]]}
    seed_dir = Path(tempfile.mkdtemp(prefix="jetpack-seed-"))
    (seed_dir / "manifest.osv").write_text('app "Seed v1 pilots" {}\n')
    (seed_dir / "main.lua").write_text(
        "local a = asset:open('jetpack-pilots')\n"
        "local function tag(v)\n\tif type(v) ~= 'table' then return v end\n\tlocal out, n = {}, 0\n"
        "\tfor k, x in pairs(v) do out[k] = tag(x); n = n + 1 end\n"
        "\treturn n == #v and doc.list(out) or doc.map(out)\nend\n"
        f"a:set({{ 'pilots' }}, tag({lua([v1, bare])}))\n"
        "a:set({ 'info' }, tag({ version = 1 }))\n"
        "return function() return ui.col({ full = true }) end\n")
    ws = r.create_workspace("Jetpack v1 pilots")
    r.select_workspace(ws["id"])
    seeder = r.create_item(ws["id"], "Seed", "app")["id"]
    r.upload_folder(seeder, seed_dir)
    r.open_item(seeder)
    r.frame(3)
    assert not r.read_console(seeder), r.read_console(seeder)
    assert r.read_data(seeder)[PILOTS]["info"]["version"] == 1
    upgraded_editor = app("Pilot editor", "jetpack_pilot_editor")
    data = r.read_data(upgraded_editor)[PILOTS]
    assert data["info"]["version"] == 2, data["info"]
    old = next(p for p in data["pilots"] if p["id"] == "scout")
    assert old["scale"] == 0.5 and old["size"] == [80, 104] and len(old["parts"]) == 13, old["size"]
    assert sorted(shapes(old)) == ["v1", "v2", "v3", "v4", "v5", "v6"], sorted(shapes(old))
    assert old["points"]["palm_gun"] == [60, 54] and old["points"]["palm_free"] == [68, 48], old["points"]
    assert label(upgraded_editor, "editor:status").endswith("ready for the game"), label(upgraded_editor, "editor:status")
    assert {p["id"] for p in data["pilots"]} == {"scout", "bare"}
    assert not r.read_console(upgraded_editor), r.read_console(upgraded_editor)
    old_game = app("Jetpack Arena", "jetpack")
    tap("match:start")
    r.frame(30)
    assert magenta(old_game) == 0 and not r.read_console(old_game), r.read_console(old_game)
    shutil.rmtree(seed_dir)
    print("jetpack editors: map and pilot editor round-trips, game import, bake, fallback and v1 upgrade passed")
