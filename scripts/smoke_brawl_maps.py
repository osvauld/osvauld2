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
most 56 textures; screenshot assets.png."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from osvauld.session import Session, shell_binary

ROOT = Path(__file__).resolve().parent.parent
SHOTS = ROOT / "shots" / "brawl"
SPAWN = (0.0, 0.0)
SHEET_SIZE = (1800, 1200)  # one page holds the whole library: 10 x 6 cells of 168 x 176 under the 44 px bar and the shell's own bar
TEXTURE_CAP = 56  # decision 11: the whole library
# What S-M3 compares: the yard's solids and platforms (maps/yard.lua) with the library's lifts.
GROUND_LIFT = 0.0
LEDGE_LIFT, LEDGE_SCALE = 10.0, 320.0 / 256.0
LEFT_TOP = -260.0


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
        print("brawl maps smoke ok")


if __name__ == "__main__":
    main()
