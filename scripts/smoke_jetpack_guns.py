"""Jetpack guns over the real path (design/jetpack-militia.md step 2): a gun made in the gun editor by real
pointer hits is what the game holds; the gun hand's glove lands on the grip and the free hand on the
support (one-handed guns leave it hanging); the bake ships the gun; with no asset the baked guns play."""
import base64
import re
import shutil
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from osvauld.session import Session, shell_binary
from osvauld.png import Image, near
from bake_jetpack import bake

ROOT = Path(__file__).resolve().parent.parent
GUNS = "asset:jetpack-guns"
MAGENTA = "#ff00ff"
GLOVE = "#e6bf80"
SIX = ["pistol", "smg", "rifle", "shotgun", "sniper", "rocket"]

# The scout's own numbers from its baked file: where the gun sits at aim 0, and its size.
_pilot = (ROOT / "demo_apps/jetpack/baked/pilots.lua").read_text()
HOLD = [float(v) for v in re.search(r"hold = \{ ([\d.]+), ([\d.]+) \}", _pilot).groups()]
SIZE = [float(v) for v in re.search(r"\bsize = \{ ([\d.]+), ([\d.]+) \}", _pilot).groups()]
SCALE = float(re.search(r"\bscale = ([\d.]+)", _pilot).group(1))
ACTOR_W = SIZE[0] * SCALE  # the actor's frame, game units; its feet line sits 24 above the bottom
ANCHOR = (ACTOR_W / 2, SIZE[1] * SCALE - 24)  # where the soldier's (x, y) is in that frame


def find(tree, identity):
    if tree.get("id") == identity:
        return tree
    for child in tree.get("children", []):
        found = find(child, identity)
        if found:
            return found


def box(shape):
    xs = [c[k] for c in shape["path"] for k in range(1, len(c), 2)]
    ys = [c[k] for c in shape["path"] for k in range(2, len(c), 2)]
    return min(xs), min(ys), max(xs) - min(xs), max(ys) - min(ys)


def closeup(image, x, y, w, h, path, zoom=4):
    """A cropped, enlarged PNG of a screenshot region, to judge the art by (nearest neighbour)."""
    import struct
    import zlib
    x0, y0, w, h = max(0, int(x)), max(0, int(y)), int(w), int(h)
    rows = []
    for yy in range(y0, min(image.height, y0 + h)):
        line = b"".join(bytes(image.rows[yy][xx * 4:xx * 4 + 4]) * zoom for xx in range(x0, min(image.width, x0 + w)))
        rows.extend([b"\x00" + line] * zoom)
    width = (len(rows[0]) - 1) // 4 if rows else 0
    def chunk(kind, data):
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body))
    png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", width, len(rows), 8, 6, 0, 0, 0)) + \
        chunk(b"IDAT", zlib.compress(b"".join(rows))) + chunk(b"IEND", b"")
    Path(path).write_bytes(png)


def gun_points(gun, facing):
    """Grip and support in the actor's frame (game units) at a flat aim, from the gun's own numbers."""
    g, k = gun["points"]["grip"], gun["scale"]
    grip = (HOLD[0] * SCALE, HOLD[1] * SCALE) if facing > 0 else ((SIZE[0] - HOLD[0]) * SCALE, HOLD[1] * SCALE)
    out = {"grip": grip}
    if "support" in gun["points"]:
        s = gun["points"]["support"]
        out["support"] = (grip[0] + facing * (s[0] - g[0]) * k, grip[1] + (s[1] - g[1]) * k)
    return out


with Session(shell_binary=shell_binary(), offscreen=(1200, 900)) as s:
    r = s.rpc
    r.signup("maker", "jetpack guns passphrase")
    ws = r.create_workspace("Jetpack guns")

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

    def shot(item):
        return Image(base64.b64decode(r.screenshot(item)["png_base64"]))

    # Test 4 (editor): seeded from the baked guns, then edited by pointer and by fields.
    editor = app("Gun editor", "jetpack_gun_editor")

    def guns():
        return r.read_data(editor)[GUNS]["guns"]

    def gun(identity="rifle"):
        return next(g for g in guns() if g["id"] == identity)

    def shapes(g):
        return {sh["id"]: sh for part in g["parts"] for sh in part["shapes"]}

    assert [g["id"] for g in guns()] == SIX, [g["id"] for g in guns()]
    assert r.read_data(editor)[GUNS]["info"]["version"] == 1
    seeded = gun()
    assert "support" in seeded["points"] and "support" not in gun("pistol")["points"]
    assert label(editor, "editor:status").endswith("ready for the game"), label(editor, "editor:status")
    canvas = rect("gun:canvas")
    zoom = canvas["h"] / seeded["size"][1]  # the element stretches across; the drawing does not
    assert zoom == 5, canvas

    def at(x, y):
        return canvas["x"] + x * zoom, canvas["y"] + y * zoom

    # A part the rifle lacks (the slide) is made by drawing into it; a rect lands in the body on top.
    assert [p["id"] for p in seeded["parts"]] == ["mag", "body", "bolt", "barrel"]
    tap("part:slide")
    tap("tool:rect")
    r.click_at(*at(10, 30))
    r.frame(2)
    drawn = gun()
    assert [p["id"] for p in drawn["parts"]] == ["mag", "body", "slide", "bolt", "barrel"], [p["id"] for p in drawn["parts"]]
    slide = next(p for p in drawn["parts"] if p["id"] == "slide")
    assert len(slide["shapes"]) == 1 and box(slide["shapes"][0]) == (10, 30, 8, 8), slide
    tap("part:body")
    r.click_at(*at(50, 33))
    r.frame(2)
    body = next(p for p in gun()["parts"] if p["id"] == "body")
    assert box(body["shapes"][-1]) == (50, 33, 8, 8) and body["shapes"][-1]["fill"] == "@metal", body["shapes"][-1]
    assert label(editor, "editor:status").endswith("ready for the game"), label(editor, "editor:status")
    tap("tool:select")

    # The muzzle marker is dragged two units back and one down; one write on release.
    mx, my = seeded["points"]["muzzle"]
    r.drag(at(mx, my), at(mx - 2, my + 1))
    r.frame(2)
    assert gun()["points"]["muzzle"] == [mx - 2, my + 1], gun()["points"]["muzzle"]

    # A stat by its field; an illegal one is refused and says why.
    r.type_text(editor, "stat:damage", "17")
    r.key(editor, "stat:damage", "enter")
    r.frame(2)
    assert gun()["stats"]["damage"] == 17, gun()["stats"]
    r.type_text(editor, "stat:damage", "5000")
    r.key(editor, "stat:damage", "enter")
    r.frame(2)
    assert gun()["stats"]["damage"] == 17 and label(editor, "editor:status").startswith("damage must be"), label(editor, "editor:status")
    r.type_text(editor, "stat:damage", "17")
    r.key(editor, "stat:damage", "enter")
    r.frame(2)
    r.type_text(editor, "stat:scale", "0.27")
    r.key(editor, "stat:scale", "enter")
    r.frame(2)
    assert gun()["scale"] == 0.27, gun()["scale"]

    # Zoom levels: the rifle seeds {1, 1.5, 2}; add a level, edit it, refuse a descending one, remove it.
    assert gun()["stats"]["zooms"] == [1, 1.5, 2] and "zoom" not in gun()["stats"], gun()["stats"]
    tap("zoom:add")
    assert gun()["stats"]["zooms"] == [1, 1.5, 2, 2.5], gun()["stats"]
    r.type_text(editor, "stat:zoom:4", "2.25")
    r.key(editor, "stat:zoom:4", "enter")
    r.frame(2)
    assert gun()["stats"]["zooms"] == [1, 1.5, 2, 2.25], gun()["stats"]
    r.type_text(editor, "stat:zoom:2", "3")
    r.key(editor, "stat:zoom:2", "enter")
    r.frame(2)
    assert gun()["stats"]["zooms"] == [1, 1.5, 2, 2.25] and label(editor, "editor:status").startswith("zoom levels must"), label(editor, "editor:status")
    r.type_text(editor, "stat:zoom:2", "1.5")  # a legal draft clears the refused one
    r.key(editor, "stat:zoom:2", "enter")
    r.frame(2)
    tap("zoom:remove")
    tap("zoom:remove")
    assert gun()["stats"]["zooms"] == [1, 1.5], gun()["stats"]
    assert not any(e["id"] == "stat:zoom:3" for e in r.rects())
    tap("zoom:add")
    assert gun()["stats"]["zooms"] == [1, 1.5, 2], gun()["stats"]

    # One-handed: the support goes and comes back; the pistol has none to begin with.
    tap("support:remove")
    assert "support" not in gun()["points"], gun()["points"]
    assert label(editor, "editor:status").endswith("ready for the game"), label(editor, "editor:status")
    tap("support:add")
    grip = gun()["points"]["grip"]
    assert gun()["points"]["support"] == [grip[0] + 22, grip[1] + 2], gun()["points"]
    tap("gun:pistol")
    assert [e["id"] for e in r.rects() if e["id"].startswith("support:")] == ["support:add"]

    # Many guns: duplicate, a new plain one, delete both.
    tap("gun:copy")
    assert len(guns()) == 7 and guns()[-1]["name"] == "Pistol copy" and guns()[-1]["id"] != "pistol", [g["id"] for g in guns()]
    tap("gun:new")
    assert len(guns()) == 8 and guns()[-1]["name"] == "New gun", [g["name"] for g in guns()]
    assert label(editor, "editor:status").endswith("ready for the game"), label(editor, "editor:status")
    copy_id = guns()[6]["id"]
    tap("gun:delete")  # the new gun, which is the one shown
    assert len(guns()) == 7, [g["id"] for g in guns()]
    tap("gun:" + copy_id)
    tap("gun:delete")
    assert [g["id"] for g in guns()] == SIX, [g["id"] for g in guns()]

    # The rifle's metal var recolours its receiver; Aim mode runs the preview with the muzzle flash.
    tap("gun:rifle")
    r.type_text(editor, "var:metal", MAGENTA)
    r.key(editor, "var:metal", "enter")
    r.frame(2)
    assert gun()["vars"]["metal"] == MAGENTA, gun()["vars"]
    edited = gun()
    assert edited["scale"] == 0.27 and edited["stats"]["damage"] == 17 and edited["points"]["support"]
    tap("motion:aim")
    r.frame(20)
    assert not r.read_console(editor), r.read_console(editor)
    r.save_screenshot(editor, str(ROOT / "shots/jetpack-gun-editor.png"))
    before = guns()
    r.request("CloseItem", item_id=editor)
    r.frame(2)
    r.open_item(editor)
    r.frame(3)
    assert guns() == before, "the edited guns did not survive closing the editor"

    def magenta(image):
        return sum(near(image.at(x, y), MAGENTA, 40) for y in range(0, image.height) for x in range(0, image.width))

    def glove_in(image, cx, cy, half):
        return sum(near(image.at(x, y), GLOVE, 12) for y in range(int(cy - half), int(cy + half) + 1)
                   for x in range(int(cx - half), int(cx + half) + 1))

    def holds(item, g, label_):
        """Aim flat right, then flat left: the glove sits on the grip, and on the support if two-handed."""
        for facing in (1, -1):
            for _ in range(4):  # the camera leads toward the aim and eases: re-aim from where the soldier settles
                top = rect("player")
                cs = float(label(item, "camera:view").split()[2])  # px per game unit
                ax, ay = top["x"] + ANCHOR[0] * cs, top["y"] + ANCHOR[1] * cs  # the soldier on screen
                r.move_to(ax + facing * 100 * cs, ay - 4 * cs)  # level with the gun: aim 0 or pi
                r.frame(30)
            top = rect("player")
            cs = float(label(item, "camera:view").split()[2])
            image = shot(item)
            pts = gun_points(g, facing)
            half = max(3, 3.2 * cs)
            sx, sy = top["x"] + pts["grip"][0] * cs, top["y"] + pts["grip"][1] * cs
            got = glove_in(image, sx, sy, half)
            print(f"{label_} facing {facing}: glove at grip {got}", end="")
            assert got >= 6, f"{label_} facing {facing}: no glove on the grip ({got})"
            if "support" in pts:
                fx, fy = top["x"] + pts["support"][0] * cs, top["y"] + pts["support"][1] * cs
                free = glove_in(image, fx, fy, half)
                print(f", at support {free}", end="")
                assert free >= 2, f"{label_} facing {facing}: no glove on the support ({free})"
            print()
            yield facing, image, top, cs

    # Test 2 (game): the game holds the edited rifle — its magenta receiver is on screen, and the
    # glove is on the grip and on the support, aiming right and left.
    game = app("Jetpack Arena", "jetpack")
    assert label(game, "map:name"), "the HUD names the arena"
    tap("match:start")
    r.frame(90)
    assert magenta(shot(game)) > 10, magenta(shot(game))
    for _ in holds(game, gun(), "edited rifle"):
        pass
    assert not r.read_console(game), r.read_console(game)

    # One-handed: the test-only loadout asset (design: a seam until step 5) gives everyone the pistol.
    # Its glove is on the grip, and where a rifle's free hand would hold, there is none.
    seed_dir = Path(tempfile.mkdtemp(prefix="jetpack-loadout-"))
    (seed_dir / "manifest.osv").write_text('app "Seed loadout" {}\n')
    (seed_dir / "main.lua").write_text(
        "local a = asset:open('jetpack-loadout')\n"
        "a:set({ 'loadout' }, doc.map({ player = 'pistol', bot = 'pistol' }))\n"
        "return function() return ui.col({ full = true }) end\n")
    seeder = r.create_item(ws["id"], "Seed", "app")["id"]
    r.upload_folder(seeder, seed_dir)
    r.open_item(seeder)
    r.frame(3)
    assert not r.read_console(seeder), r.read_console(seeder)
    shutil.rmtree(seed_dir)
    pistol_game = app("Jetpack Arena", "jetpack")
    tap("match:start")
    r.frame(90)
    pistol = gun("pistol")
    rifle_hold = gun_points(gun(), 1)["support"]
    for facing, image, top, cs in holds(pistol_game, pistol, "pistol"):
        side = gun_points(gun(), facing)["support"]  # clear of the gun hand, which is ~4 units across
        free = glove_in(image, top["x"] + (side[0] + facing * 4) * cs, top["y"] + side[1] * cs, 2.5 * cs)
        assert free == 0, f"a one-handed pistol left a hand out at the rifle's support ({free} glove pixels)"
    assert not r.read_console(pistol_game), r.read_console(pistol_game)

    # Test 5: the bake ships the gun; a game with no asset plays it.
    baked_root = Path(tempfile.mkdtemp(prefix="jetpack-bake-"))
    shutil.copytree(ROOT / "demo_apps" / "jetpack", baked_root / "demo_apps" / "jetpack")
    written = bake(r, [editor], baked_root)
    assert baked_root / "demo_apps/jetpack/baked/guns.lua" in written and len(written) == 4, written
    assert bake(r, [editor], baked_root) == written, "re-bake must be stable"
    assert "#ff00ff" in (baked_root / "demo_apps/jetpack/baked/guns.lua").read_text()

    ws = r.create_workspace("Jetpack baked")
    r.select_workspace(ws["id"])
    shipped = app("Jetpack shipped", "jetpack", baked_root)
    tap("match:start")
    r.frame(90)
    assert magenta(shot(shipped)) > 10, "the baked copy holds the edited gun"
    assert not r.read_console(shipped), r.read_console(shipped)
    shutil.rmtree(baked_root)

    # Test 6: with no asset and the repo's baked guns, the soldier holds the baked rifle (no magenta); the
    # glove is on its grip and support. This is the screenshot to judge the art by.
    plain = app("Jetpack Arena", "jetpack")
    tap("match:start")
    r.frame(90)
    assert magenta(shot(plain)) == 0, "the repo's baked rifle has a steel receiver"
    for facing, image, top, cs in holds(plain, seeded, "baked rifle"):
        name = "jetpack-guns-game.png" if facing > 0 else "jetpack-guns-game-left.png"
        r.save_screenshot(plain, str(ROOT / "shots" / name))
        closeup(image, top["x"] - (10 if facing > 0 else 55) * cs, top["y"] - 8 * cs, 115 * cs, 72 * cs,
                ROOT / "shots" / name.replace("game", "closeup"), zoom=2)
    assert not r.read_console(plain), r.read_console(plain)
    print("jetpack guns: editor round-trip, game holds the gun, one- and two-handed grips, bake and baked fallback passed")
