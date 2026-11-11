"""Brawl's yard on the asset library (plan: docs/design/brawl-maps.md §0, step 2). S-M1: upload and
open, the console is clean, `map:state` reads `yard`, `art:loaded` reaches n/n, the pictures in the
dump are exactly the ones `map:art` (the compiled scene's) names, and the fighter stands at the spawn
of maps/yard.lua. Screenshots maps-yard.png (standing on the ground) and maps-yard-platform.png (on
the left platform, the robot's soles on the drawn grass) go to shots/brawl/ for the user to compare
with step5_*.png: the same yard, the ground's grass 3 units lower and the platform's grass 7.5 units
higher (the pictures' lifts now come from the library).

Step 3: S-M3, the collision overlay (hidden button `overlay`, key F2): `col:<id>` for every solid and
platform, each matching its picture frame `piece:<id>` within 1 px (a ground's and a ledge's frame is
the collision width, and its picture's first opaque row, `lift` x scale, sits on the collision top);
screenshot maps-yard-overlay.png. S-M2, the asset sheet, in a larger window so every cell and name is
legible: clicking `assets` makes `gallery:state` read n/n with n the library's size, the dump holds at
most 56 textures; screenshot assets.png.

Step 4: S-M6, backgrounds at the edges. The fighter is driven to the left and right ends of the ground,
off the right end (falling past the edge, where step 3 showed sky under the hedge), and onto the top
platform (the respawn lands there); then the hidden `wide` button pulls the camera all the way out,
with `pair` drawing a second robot. In every view the playfield's bottom row is never the sky's top
colour, nor its bottom colour (the bare sky). Screenshots maps-yard-{idle,edge,top,wide}.png.

Step 5: S-M4, map switching. `map:next` makes `map:state` read isles, puts the fighter at isles' spawn
(-700, 0) and leaves the console clean; four switches in all keep the dump's texture count within the
library's (56) and the console clean. The fighter is driven over the boulder to the bridge, through it
into the pit and out to the top platform: screenshots maps-isles-{idle,pit,top,wide}.png. S-M5, live edit
(hillclimb style): read_file_versioned and one edit_file move yard's `right` ledge by +64 x; after the
reload the fighter, jumping at x 650 (only the new place), lands on it and `piece:right` has moved by
64 px a unit. A broken edit (asset "wood_ledgee") puts the error, naming the piece, on the console;
fixing it brings a clean console back."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from osvauld.png import Image, near
from osvauld.session import Session, shell_binary

ROOT = Path(__file__).resolve().parent.parent
SHOTS = ROOT / "shots" / "brawl"
SPAWN = (0.0, 0.0)
SHEET_SIZE = (1800, 1200)  # one page holds the whole library: 10 x 6 cells of 168 x 176 under the 44 px bar and the shell's own bar
TEXTURE_CAP = 56  # decision 11: the whole library
# What S-M3 compares: the yard's solids and platforms (maps/yard.lua) with the library's lifts.
GROUND_LIFT = 24.0  # grass_block: tufts above the surface (step 4)
LEDGE_LIFT, LEDGE_SCALE = 10.0, 320.0 / 256.0
LEFT_TOP = -260.0
TOP_TOP = -520.0
GROUND_END = 900.0
# demo_apps/brawl/maps/assets/sky.lua: the day sky's first and last stops. A bottom row of either is
# bare sky under the stage.
SKY_TOP, SKY_BOTTOM = "#5aaed0", "#eef5df"


def find(tree, identity):
    if isinstance(tree, dict):
        if tree.get("id") == identity:
            return tree
        tree = tree.get("children", [])
    for child in tree:
        found = find(child, identity)
        if found:
            return found


def library_size():
    """The assets in maps/assets/*.lua, counted by their `kind =` (a ground's pictures have none)."""
    return sum(p.read_text().count("kind =") for p in (ROOT / "demo_apps/brawl/maps/assets").glob("*.lua"))


def textures(r, item):
    """Distinct picture files in the dump, and their bytes."""
    out = {}

    def walk(n):
        if isinstance(n, dict):
            for p in n.get("pictures", []):
                out[p["path"]] = p["bytes"]
            n = n.get("children", [])
        for c in n:
            walk(c)
    walk(r.dump_tree(item))
    return out


def open_brawl(s):
    r = s.rpc
    r.signup("brawler", "brawl maps smoke passphrase")
    ws = r.create_workspace("Brawl maps smoke")
    item = r.create_item(ws["id"], "Brawl", "app")["id"]
    r.upload_folder(item, ROOT / "demo_apps/brawl")
    r.open_item(item)
    r.frame(2)
    return r, item


def click_id(r, name):
    """A click where rects() says the element is: never a guessed coordinate."""
    box = {x["id"]: x for x in r.rects()}[name]
    r.click_at(box["x"] + box["w"] / 2, box["y"] + box["h"] / 2)
    r.frame(2)


def sheet():
    """S-M2: the asset sheet, in a window big enough for one page."""
    with Session(shell_binary=shell_binary(), offscreen=SHEET_SIZE) as s:
        r, item = open_brawl(s)
        click_id(r, "assets")
        want = library_size()
        for _ in range(400):
            node = find(r.dump_tree(item), "gallery:state")
            assert node, r.read_console(item)
            n, total = node["text"].split(" ")[0].split("/")
            if n == total:
                break
            r.frame(1)
        assert n == total and int(total) == want, (node["text"], want)
        r.frame(2)
        console = r.read_console(item)
        assert not console, console
        listed = textures(r, item)
        assert 0 < len(listed) <= TEXTURE_CAP, (len(listed), sorted(listed))
        mib = sum(listed.values()) / 1048576
        r.save_screenshot(item, str(SHOTS / "assets.png"))
        print(f"S-M2 sheet: gallery {n}/{total} for {want} assets, {len(listed)} textures, {mib:.1f} MiB of pictures in the dump")
        click_id(r, "assets")  # and back to the map
        assert find(r.dump_tree(item), "map:state"), "the map is back"


def main():
    sheet()
    SHOTS.mkdir(parents=True, exist_ok=True)
    with Session(shell_binary=shell_binary(), offscreen=(900, 700)) as s:
        r = s.rpc
        r.signup("brawler", "brawl maps smoke passphrase")
        ws = r.create_workspace("Brawl maps smoke")
        item = r.create_item(ws["id"], "Brawl", "app")["id"]
        r.upload_folder(item, ROOT / "demo_apps/brawl")
        r.open_item(item)
        r.frame(2)

        def text(name):
            node = find(r.dump_tree(item), name)
            assert node, (name, r.read_console(item))
            return node["text"]

        def state():
            return {k: (v if k == "act" else float(v)) for k, v in (p.split("=") for p in text("fighter:state").split())}

        def pictures():
            out = {}

            def walk(n):
                if isinstance(n, dict):
                    for p in n.get("pictures", []):
                        out[p["path"]] = out.get(p["path"], 0) + p["bytes"]
                    n = n.get("children", [])
                for c in n:
                    walk(c)
            walk(r.dump_tree(item))
            return out

        for _ in range(60):
            n, total = text("art:loaded").split("/")
            if n == total:
                break
            r.frame(1)
        assert n == total and int(total) > 0, text("art:loaded")
        console = r.read_console(item)
        assert not console, console
        assert text("map:state") == "yard", text("map:state")
        named = set(text("map:art").split())
        png = {p for p in named if p.endswith(".png")}
        listed = pictures()
        assert set(listed) == png and all(b > 0 for b in listed.values()), (sorted(set(listed) ^ png), listed)
        print(f"S-M1 yard: art {n}/{total}, {len(listed)} pictures, {sum(listed.values())} bytes, map:state yard")
        for _ in range(4):
            r.frame(1)
        st = state()
        assert (st["x"], st["y"]) == SPAWN and st["vy"] == 0 and st["stocks"] == 3 and st["act"] == "idle", st
        r.save_screenshot(item, str(SHOTS / "maps-yard.png"))
        r.save_screenshot(item, str(SHOTS / "maps-yard-idle.png"))

        # S-M3: the overlay. Off, there is no `col:`; on, one per solid and platform.
        def ids():
            return {x["id"]: x for x in r.rects()}
        assert not [i for i in ids() if i.startswith("col:")], "the overlay starts off"
        click_id(r, "overlay")
        k = float(text("camera:view").split("w=")[1].split()[0])
        k = 900.0 / k
        rects = ids()
        pieces = {"ground": ("solid", GROUND_LIFT, 1.0), "left": ("platform", LEDGE_LIFT, LEDGE_SCALE),
                  "right": ("platform", LEDGE_LIFT, LEDGE_SCALE), "top": ("platform", LEDGE_LIFT, LEDGE_SCALE)}
        assert sorted(i[4:] for i in rects if i.startswith("col:")) == sorted(pieces), sorted(i for i in rects if i.startswith("col:"))
        for name, (kind, lift, scale) in pieces.items():
            c, p = rects["col:" + name], rects["piece:" + name]
            assert abs(c["x"] - p["x"]) <= 1 and abs(c["w"] - p["w"]) <= 1, (name, c, p)
            assert abs(c["y"] - (p["y"] + lift * scale * k)) <= 1, (name, c, p, k)  # the picture's surface is the collision top
            if kind == "solid":
                assert p["h"] >= c["h"] - 1, (name, c, p)  # the body runs on to the bottom of the view
        print("S-M3 overlay: col:<id> for", ", ".join(sorted(pieces)), "each on its piece frame within 1 px")
        r.save_screenshot(item, str(SHOTS / "maps-yard-overlay.png"))
        r.keyboard("F2", "F2", True)  # F2 toggles it too
        r.keyboard("F2", "F2", False)
        r.frame(2)
        assert not [i for i in ids() if i.startswith("col:")], "F2 turns the overlay off"

        # The left platform, as smoke_brawl's S-4: run under it, jump with Space held, land on it.
        r.keyboard("ArrowLeft", "ArrowLeft", True)
        for _ in range(120):
            r.frame(1)
            if state()["x"] < -420:
                break
        r.keyboard("ArrowLeft", "ArrowLeft", False)
        for _ in range(30):
            r.frame(1)
            if state()["vx"] == 0:
                break
        r.keyboard("Space", " ", True)
        for _ in range(30):
            r.frame(1)
        r.keyboard("Space", " ", False)
        for _ in range(80):
            r.frame(1)
            if state()["act"] == "idle" and state()["tick"] > 0:
                break
        for _ in range(10):
            r.frame(1)
        st = state()
        assert st["y"] == LEFT_TOP and st["vy"] == 0 and st["act"] == "idle", st
        r.save_screenshot(item, str(SHOTS / "maps-yard-platform.png"))
        console = r.read_console(item)
        assert not console, console
        backgrounds(r, item, text, state)
    switching()
    live_edit()
    print("brawl maps smoke ok")


def backgrounds(r, item, text, state):
    """S-M6: the playfield's bottom row is never bare sky, at the ground's ends, past the edge, on the top
    platform and zoomed all the way out."""
    def frames(n, until=None):
        for _ in range(n):
            r.frame(1)
            if until and until(state()):
                break
        return state()

    def hold(code, key, until, n=300):
        r.keyboard(code, key, True)
        st = frames(n, until)
        r.keyboard(code, key, False)
        return st

    def settle():
        """Until the camera has stopped easing."""
        prev = None
        for _ in range(240):
            r.frame(1)
            cam = text("camera:view")
            if cam == prev:
                break
            prev = cam

    def check(label, shot=None):
        path = SHOTS / (shot or "maps-yard-check.png")
        r.save_screenshot(item, str(path))
        img = Image(path.read_bytes())
        # The playfield runs to the window's bottom and across it (the shell's bar is above it).
        y = img.height - 1
        xs = range(1, img.width - 1, 3)
        row = [img.at(x, y) for x in xs]
        top = sum(near(c, SKY_TOP, 20) for c in row)
        bottom = sum(near(c, SKY_BOTTOM, 10) for c in row)
        st = state()
        print(f"S-M6 {label}: x={st['x']:g} y={st['y']:g} act={st['act']} camera {text('camera:view').split(' zoom_min')[0]}; "
              f"bottom row {len(row)} px: {top} sky-top, {bottom} sky-bottom")
        assert top == 0 and bottom == 0, (label, "bare sky in the bottom row", top, bottom, row[:4])
        if not shot:
            path.unlink()

    # Down through the left platform, then to the ground's left end, stopping short of it.
    r.keyboard("ArrowDown", "ArrowDown", True)
    r.frame(2)
    r.keyboard("ArrowDown", "ArrowDown", False)
    st = frames(120, lambda st: st["y"] == 0 and st["act"] == "idle")
    assert st["y"] == 0, st
    st = hold("ArrowLeft", "ArrowLeft", lambda st: st["x"] < -GROUND_END + 120)
    st = frames(60, lambda st: st["vx"] == 0)
    assert st["y"] == 0 and -GROUND_END < st["x"] < -GROUND_END + 200, st
    settle()
    check("left end")
    # Across to the right end.
    st = hold("ArrowRight", "ArrowRight", lambda st: st["x"] > GROUND_END - 120, 400)
    st = frames(60, lambda st: st["vx"] == 0)
    assert st["y"] == 0 and GROUND_END - 200 < st["x"] < GROUND_END, st
    settle()
    check("right end")
    # Off it: falling past the edge, the view where step 3 showed sky under the hedge.
    st = hold("ArrowRight", "ArrowRight", lambda st: st["act"] == "fall" and st["y"] > 120, 200)
    assert st["x"] > GROUND_END and st["y"] > 0, st
    check("past the edge", "maps-yard-edge.png")
    # Out and back: the respawn falls onto the top platform.
    st = frames(600, lambda st: st["y"] == TOP_TOP and st["act"] == "idle")
    assert st["y"] == TOP_TOP and st["act"] == "idle" and st["stocks"] == 2, st
    settle()
    check("top platform", "maps-yard-top.png")
    # All the way out, two robots drawn.
    for name in ("wide", "pair"):
        box = {x["id"]: x for x in r.rects()}[name]
        r.click_at(box["x"] + box["w"] / 2, box["y"] + box["h"] / 2)
    settle()
    cam = dict(p.split("=") for p in text("camera:view").split())
    assert abs(float(cam["zoom"]) - float(cam["zoom_min"])) < 1e-6, cam
    check("wide", "maps-yard-wide.png")
    console = r.read_console(item)
    assert not console, console


ISLES_SPAWN = (-700.0, 0.0)
ISLES_SKY = ("#4a64a0", "#f9e0b2")  # the golden sky's first and last stops
YARD_MAP = "maps/yard.lua"


class Played:
    """The helpers every step of a session needs: text readouts, the fighter's state, frames."""

    def __init__(self, r, item):
        self.r, self.item = r, item

    def text(self, name):
        node = find(self.r.dump_tree(self.item), name)
        assert node, (name, self.r.read_console(self.item))
        return node["text"]

    def state(self):
        return {k: (v if k == "act" else float(v)) for k, v in (p.split("=") for p in self.text("fighter:state").split())}

    def loaded(self, limit=400):
        for _ in range(limit):
            n, total = self.text("art:loaded").split("/")
            if n == total:
                return total
            self.r.frame(1)
        raise AssertionError(self.text("art:loaded"))

    def frames(self, n, until=None):
        for _ in range(n):
            self.r.frame(1)
            if until and until(self.state()):
                break
        return self.state()

    def hold(self, code, key, until, n=300):
        self.r.keyboard(code, key, True)
        st = self.frames(n, until)
        self.r.keyboard(code, key, False)
        return st

    def tap(self, code, key, n=2):
        self.r.keyboard(code, key, True)
        self.r.frame(n)
        self.r.keyboard(code, key, False)

    def settle(self):
        prev = None
        for _ in range(240):
            self.r.frame(1)
            cam = self.text("camera:view")
            if cam == prev:
                break
            prev = cam

    def shot(self, name, bare=None):
        """A screenshot; with `bare` (the sky's two end colours), the bottom row is never bare sky (S-M6)."""
        path = SHOTS / name
        self.r.save_screenshot(self.item, str(path))
        if bare:
            img = Image(path.read_bytes())
            row = [img.at(x, img.height - 1) for x in range(1, img.width - 1, 3)]
            n = sum(near(c, bare[0], 20) or near(c, bare[1], 10) for c in row)
            assert n == 0, (name, "bare sky in the bottom row", n)


def switching():
    """S-M4: map:next cycles the index; isles' spawn, a clean console and the texture count every time."""
    with Session(shell_binary=shell_binary(), offscreen=(900, 700)) as s:
        r, item = open_brawl(s)
        g = Played(r, item)
        g.loaded()
        assert g.text("map:state") == "yard"
        seen = []
        for i, want in enumerate(["isles", "yard", "isles", "yard"]):
            click_id(r, "map:next")
            g.loaded()
            r.frame(4)
            assert g.text("map:state") == want, (i, g.text("map:state"))
            console = r.read_console(item)
            assert not console, (want, console)
            listed = textures(r, item)
            assert 0 < len(listed) <= TEXTURE_CAP, (want, len(listed))
            seen.append((want, len(listed), sum(listed.values()) / 1048576))
            if want == "isles" and i == 0:
                st = g.state()
                assert (st["x"], st["y"]) == ISLES_SPAWN and st["act"] == "idle" and st["stocks"] == 3, st
                g.settle()
                g.shot("maps-isles-idle.png", ISLES_SKY)
                isles_play(g)
        click_id(r, "map:next")  # a fifth: isles again, its art loads the same
        g.loaded()
        assert g.text("map:state") == "isles"
        print("S-M4 switch:", ", ".join(f"{m} {n} textures {mib:.1f} MiB" for m, n, mib in seen))
        console = r.read_console(item)
        assert not console, console


def isles_play(g):
    """Over the boulder to the bridge, down through it into the pit, then the respawn on the top platform."""
    r = g.r
    # Hold right and hop the boulder (x -555..-405) with a held jump; land on the bridge or the island.
    r.keyboard("ArrowRight", "ArrowRight", True)
    g.frames(120, lambda st: st["x"] > -690)
    r.keyboard("Space", " ", True)
    g.frames(40)
    r.keyboard("Space", " ", False)
    st = g.frames(300, lambda st: st["act"] != "fall" and st["vy"] == 0 and st["y"] == 0 and st["x"] > -200)
    r.keyboard("ArrowRight", "ArrowRight", False)
    st = g.frames(60, lambda st: st["vx"] == 0)
    assert st["y"] == 0 and -260 < st["x"] < 260, ("on the bridge", st)
    # Down drops through it; the pit takes the fighter: shoot it falling between the islands.
    g.tap("ArrowDown", "ArrowDown")
    st = g.frames(60, lambda st: st["y"] > 250)
    assert st["y"] > 250, ("falling in the pit", st)
    g.shot("maps-isles-pit.png", ISLES_SKY)
    st = g.frames(600, lambda st: st["act"] == "out")
    assert st["stocks"] == 2, st
    st = g.frames(600, lambda st: st["y"] == -420 and st["act"] != "out" and st["vy"] == 0)
    assert st["y"] == -420 and st["stocks"] == 2, ("respawn on the top platform", st)
    g.settle()
    g.shot("maps-isles-top.png", ISLES_SKY)
    click_id(r, "wide")  # all the way out: both islands, the pit, every layer's edge
    g.settle()
    g.shot("maps-isles-wide.png", ISLES_SKY)
    click_id(r, "wide")
    console = r.read_console(g.item)
    assert not console, console


def live_edit():
    """S-M5: an agent's edit to the map file over the bridge moves the ledge live; a broken one is named."""
    with Session(shell_binary=shell_binary(), offscreen=(900, 700)) as s:
        r, item = open_brawl(s)
        g = Played(r, item)
        g.loaded()

        def px():
            """The right ledge's offset from the ground's left edge, in px."""
            boxes = {x["id"]: x for x in r.rects()}
            return boxes["piece:right"]["x"] - boxes["piece:ground"]["x"]

        g.settle()
        before = px()
        k = 900.0 / float(g.text("camera:view").split("w=")[1].split()[0])
        src = r.read_file_versioned(item, YARD_MAP)
        old = 'id = "right", x = 300,'
        r.edit_file(item, YARD_MAP, src["revision"], [{"old_text": old, "new_text": 'id = "right", x = 364,'}])
        r.frame(30)
        console = r.read_console(item)
        assert not console, console
        g.loaded()
        g.settle()
        moved = px() - before
        assert abs(moved - 64 * k) <= 1.5, ("piece:right moved 64 units", moved, 64 * k)
        # x 650 is past the old ledge (300..620) and under the new one (364..684): the jump lands on it.
        st = g.hold("ArrowRight", "ArrowRight", lambda st: st["x"] >= 640, 400)
        st = g.frames(60, lambda st: st["vx"] == 0)
        g.tap("Space", " ", 30)
        st = g.frames(200, lambda st: st["y"] == -260 and st["vy"] == 0 and st["act"] != "fall")
        assert st["y"] == -260 and st["act"] in ("idle", "land", "run") and st["x"] > 620, ("on the moved ledge", st)
        print(f"S-M5 live edit: piece:right moved {moved:.1f} px (64 units = {64 * k:.1f}), the fighter stands on it at x {st['x']:g}")
        # A broken edit: the error names the piece, the old app keeps running; the fix is clean.
        logged = len(r.read_console(item))
        src = r.read_file_versioned(item, YARD_MAP)
        r.edit_file(item, YARD_MAP, src["revision"], [{"old_text": 'asset = "wood_ledge", id = "right"', "new_text": 'asset = "wood_ledgee", id = "right"'}])
        r.frame(30)
        console = r.read_console(item)
        said = " ".join(str(line) for line in console)
        assert 'pieces[9] "right"' in said and "wood_ledgee" in said, console
        src = r.read_file_versioned(item, YARD_MAP)
        r.edit_file(item, YARD_MAP, src["revision"], [{"old_text": 'asset = "wood_ledgee", id = "right"', "new_text": 'asset = "wood_ledge", id = "right"'}])
        r.frame(30)
        g.loaded()
        after = r.read_console(item)
        assert len(after) == len(console), ("the fix adds nothing to the console", after[len(console):])
        print("S-M5 broken edit named:", said[:140])


if __name__ == "__main__":
    main()
