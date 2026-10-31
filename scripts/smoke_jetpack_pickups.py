"""Jetpack weapon spawns, pickups and swap over the real path (design/jetpack-militia.md step 5, test 5).

The map editor places, edits (gun pool, respawn time), moves and deletes weapon spawns by real pointer hits;
the game plays the edited yard: the gun lies where it was placed, standing over it the HUD names it and the
pickup button (found through `rects`, never guessed) appears; pressing it takes the gun (the old one drops),
and the swap button toggles the two carried guns. Then the same flow at 960x540 and 780x440 (a landscape phone) on a seeded
asset, with screenshots in shots/jetpack-pickups*.png."""
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from osvauld.session import Session, shell_binary
from bake_jetpack import lua

ROOT = Path(__file__).resolve().parent.parent
ASSET = "asset:jetpack-maps"


def find(tree, identity):
    if tree.get("id") == identity:
        return tree
    for child in tree.get("children", []):
        found = find(child, identity)
        if found:
            return found


def by_id(items):
    return {e["id"]: e for e in items}


def overlap(a, b):
    return a["x"] < b["x"] + b["w"] and b["x"] < a["x"] + a["w"] and a["y"] < b["y"] + b["h"] and b["y"] < a["y"] + a["h"]


def camera_scale(label, game):
    """px per world unit, from the invisible `camera:view` text ("x y scale")."""
    return float(label(game, "camera:view").split()[2])


def max_zoom(playfield):
    """Camera.max_zoom: the widest view that keeps a 52-unit soldier 30 px tall."""
    w, h = playfield["w"], playfield["h"]
    return max(1.0, min(w, h) * 52 / (30 * (210 if w >= h else 240)))


def helpers(r, ws):
    def app(name, folder):
        item = r.create_item(ws["id"], name, "app")["id"]
        r.upload_folder(item, ROOT / "demo_apps" / folder)
        r.open_item(item)
        r.frame(3)
        assert not r.read_console(item), r.read_console(item)
        return item

    def rect(identity):
        found = [e for e in r.rects() if e["id"] == identity]
        assert found, (identity, sorted(e["id"] for e in r.rects()))
        return found[0]

    def has(identity):
        return any(e["id"] == identity for e in r.rects())

    def tap(identity):
        r.click_at(*r.centre_of(identity))
        r.frame(2)

    def label(item, identity):
        node = find(r.dump_tree(item), identity)
        return node["text"] if node else None

    return app, rect, has, tap, label


def play(r, ws, tag, size):
    """The game on the workspace's edited yard: walk the pickup / swap flow and leave screenshots."""
    app, rect, has, tap, label = helpers(r, ws)
    game = app("Jetpack Arena", "jetpack")
    tap("map:pick:yard")
    assert label(game, "map:name") == "Training Yard"
    tap("match:start")
    r.frame(120)  # the soldier lands (2 s)
    shot = lambda name: r.save_screenshot(game, str(ROOT / f"shots/jetpack-pickups-{tag}-{name}.png"))
    assert label(game, "hud:gun") == "RIFLE", label(game, "hud:gun")
    assert label(game, "hud:other") == "Q  PISTOL 12", label(game, "hud:other")
    assert label(game, "hud:ammo") == "30/30"

    # Standing at the placed gun: the HUD names it and the pickup button is on screen, beside swap, off the sticks.
    assert label(game, "hud:pickup") == "PICK UP SHOTGUN", label(game, "hud:pickup")
    button, swap = rect("button:pickup"), rect("button:swap")
    assert button["y"] == swap["y"] and button["x"] + button["w"] <= swap["x"], (button, swap)
    for stick in ("control:move", "control:aim", "button:reload"):
        assert not overlap(button, rect(stick)) and not overlap(swap, rect(stick)), stick
    shot("prompt")

    # Pick it up: the shotgun is in hand, the rifle lies where the shotgun lay, the pistol stays on the back.
    tap("button:pickup")
    assert label(game, "hud:gun") == "SHOTGUN", label(game, "hud:gun")
    assert label(game, "hud:ammo") == "6/6", label(game, "hud:ammo")
    assert label(game, "hud:other") == "Q  PISTOL 12", label(game, "hud:other")
    assert label(game, "hud:pickup") == "PICK UP RIFLE", label(game, "hud:pickup")
    assert has("button:pickup")

    # Swap toggles the two carried guns (after the 0.3 s it takes to equip), each keeping its own mag.
    r.frame(30)
    tap("button:swap")
    assert label(game, "hud:gun") == "PISTOL", label(game, "hud:gun")
    assert label(game, "hud:ammo") == "12/12"
    assert label(game, "hud:other") == "Q  SHOTGUN 6", label(game, "hud:other")
    tap("button:swap")
    assert label(game, "hud:gun") == "PISTOL", "a swap inside the swap time is ignored"
    r.frame(30)
    shot("swapped")
    tap("button:swap")
    assert label(game, "hud:gun") == "SHOTGUN", label(game, "hud:gun")
    assert label(game, "hud:ammo") == "6/6"

    # Q swaps and E picks up too.
    r.frame(30)
    r.keyboard("KeyQ", "q", True)
    r.keyboard("KeyQ", "q", False)
    r.frame(2)
    assert label(game, "hud:gun") == "PISTOL", label(game, "hud:gun")
    r.frame(30)
    r.keyboard("KeyE", "e", True)
    r.keyboard("KeyE", "e", False)
    r.frame(2)
    assert label(game, "hud:gun") == "RIFLE", label(game, "hud:gun")
    assert label(game, "hud:other") == "Q  SHOTGUN 6", label(game, "hud:other")

    assert int(label(game, "hud:health").split()[-1]) > 0, "the player died mid-flow: " + label(game, "hud:health")
    assert not r.read_console(game), r.read_console(game)
    shot("rifle")
    return game


def run_editor():
    """Place, edit, move and delete weapon spawns in the map editor by pointer; returns the edited yard."""
    with Session(shell_binary=shell_binary(), offscreen=(1200, 900)) as s:
        r = s.rpc
        r.signup("maker", "jetpack pickups passphrase")
        ws = r.create_workspace("Jetpack pickups")
        app, rect, has, tap, label = helpers(r, ws)
        editor = app("Map editor", "jetpack_map_editor")

        def yard():
            return next(m for m in r.read_data(editor)[ASSET]["maps"] if m["id"] == "yard")

        assert "weapon_spawns" not in yard() or yard()["weapon_spawns"] == [], "the yard starts without weapon spawns"
        canvas = rect("map:canvas")

        def screen(x, y):
            return canvas["x"] + x, canvas["y"] + y

        # Placing: a click with the weapon tool adds a spawn offering the first gun, respawning after 30 s.
        tap("tool:weapon")
        r.click_at(*screen(140, 440))
        r.frame(2)
        spawns = yard()["weapon_spawns"]
        assert len(spawns) == 1 and (spawns[0]["x"], spawns[0]["y"]) == (140, 440), spawns
        assert spawns[0]["pool"] == ["pistol"] and spawns[0]["respawn"] == 30, spawns[0]
        first = spawns[0]["id"]
        assert label(editor, "selected:kind") == "WEAPON SPAWN", label(editor, "selected:kind")
        assert label(editor, "pool:pistol:count") == "1" and label(editor, "pool:shotgun:count") == "0"
        # The pool: one more shotgun, then the pistol out (the last gun cannot be removed).
        tap("pool:shotgun:plus")
        tap("pool:pistol:minus")
        assert next(w for w in yard()["weapon_spawns"] if w["id"] == first)["pool"] == ["shotgun"], yard()["weapon_spawns"]
        tap("pool:shotgun:minus")
        assert next(w for w in yard()["weapon_spawns"] if w["id"] == first)["pool"] == ["shotgun"]
        assert label(editor, "editor:status") == "A weapon spawn needs at least one gun", label(editor, "editor:status")
        tap("pool:shotgun:plus")
        assert next(w for w in yard()["weapon_spawns"] if w["id"] == first)["pool"] == ["shotgun", "shotgun"]
        assert label(editor, "pool:shotgun:count") == "2"
        tap("pool:shotgun:minus")
        r.type_text(editor, "weapon:respawn", "5")
        r.key(editor, "weapon:respawn", "enter")
        r.frame(2)
        assert next(w for w in yard()["weapon_spawns"] if w["id"] == first)["respawn"] == 5
        # Out of range is refused and says why.
        r.type_text(editor, "weapon:respawn", "0")
        r.key(editor, "weapon:respawn", "enter")
        r.frame(2)
        assert next(w for w in yard()["weapon_spawns"] if w["id"] == first)["respawn"] == 5
        assert label(editor, "editor:status").startswith("Respawn is"), label(editor, "editor:status")
        r.type_text(editor, "weapon:respawn", "5")
        r.key(editor, "weapon:respawn", "enter")
        r.frame(2)

        # A second one on the first ledge, offering the smg, and a third that is deleted again.
        r.click_at(*screen(300, 310))
        r.frame(2)
        second = [w for w in yard()["weapon_spawns"] if w["id"] != first]
        assert len(second) == 1 and (second[0]["x"], second[0]["y"]) == (300, 310), second
        tap("pool:smg:plus")
        tap("pool:pistol:minus")
        r.click_at(*screen(800, 440))
        r.frame(2)
        assert len(yard()["weapon_spawns"]) == 3
        tap("selected:delete")
        assert len(yard()["weapon_spawns"]) == 2, yard()["weapon_spawns"]

        # Moving: select, drag; one write on release, snapped to 10.
        tap("tool:select")
        r.drag(screen(140, 440), screen(151, 440))
        r.frame(2)
        moved = next(w for w in yard()["weapon_spawns"] if w["id"] == first)
        assert (moved["x"], moved["y"]) == (150, 440), moved
        assert label(editor, "editor:status").endswith("ready for the game"), label(editor, "editor:status")
        assert "2 weapon spawns" in label(editor, "editor:status"), label(editor, "editor:status")
        assert not r.read_console(editor), r.read_console(editor)
        r.save_screenshot(editor, str(ROOT / "shots/jetpack-pickups-editor.png"))
        # Reopened from the vault, the same spawns.
        edited = yard()
        r.request("CloseItem", item_id=editor)
        r.frame(2)
        r.open_item(editor)
        r.frame(3)
        assert yard() == edited, "the weapon spawns did not survive closing the editor"

        # The game, in the same workspace, plays the edited yard (the asset wins over the baked one).
        play(r, ws, "1200x900", (1200, 900))
        return edited


def run_seeded(edited, size):
    """The same flow on a workspace whose map asset is seeded with the edited yard (no editor UI needed)."""
    tag = f"{size[0]}x{size[1]}"
    seed = Path(tempfile.mkdtemp(prefix="jetpack-seed-"))
    (seed / "manifest.osv").write_text('app "Seed maps" {}\n')
    (seed / "main.lua").write_text(
        "local a = asset:open('jetpack-maps')\n"
        "local function tag(v)\n\tif type(v) ~= 'table' then return v end\n\tlocal out, n = {}, 0\n"
        "\tfor k, x in pairs(v) do out[k] = tag(x); n = n + 1 end\n"
        "\treturn n == #v and doc.list(out) or doc.map(out)\nend\n"
        f"a:set({{ 'maps' }}, tag({lua([edited])}))\n"
        "a:set({ 'info' }, tag({ version = 1 }))\n"
        "return function() return ui.col({ full = true }) end\n")
    with Session(shell_binary=shell_binary(), offscreen=size) as s:
        r = s.rpc
        r.signup("maker", "jetpack pickups passphrase")
        ws = r.create_workspace("Jetpack pickups " + tag)
        seeder = r.create_item(ws["id"], "Seed", "app")["id"]
        r.upload_folder(seeder, seed)
        r.open_item(seeder)
        r.frame(3)
        assert not r.read_console(seeder), r.read_console(seeder)
        play(r, ws, tag, size)


def run_sniper(size):
    """The sniper's four scope levels through the loadout test seam: the camera widens per tap and wraps; screenshots."""
    tag = f"{size[0]}x{size[1]}"
    seed = Path(tempfile.mkdtemp(prefix="jetpack-loadout-"))
    (seed / "manifest.osv").write_text('app "Seed loadout" {}\n')
    (seed / "main.lua").write_text(
        "local a = asset:open('jetpack-loadout')\n"
        "a:set({ 'loadout' }, doc.map({ player = 'sniper', bot = 'pistol' }))\n"
        "return function() return ui.col({ full = true }) end\n")
    with Session(shell_binary=shell_binary(), offscreen=size) as s:
        r = s.rpc
        r.signup("maker", "jetpack zoom passphrase")
        ws = r.create_workspace("Jetpack zoom " + tag)
        app, rect, has, tap, label = helpers(r, ws)
        seeder = r.create_item(ws["id"], "Seed", "app")["id"]
        r.upload_folder(seeder, seed)
        r.open_item(seeder)
        r.frame(3)
        assert not r.read_console(seeder), r.read_console(seeder)
        game = app("Jetpack Arena", "jetpack")
        tap("map:pick:yard")
        tap("match:start")
        r.frame(120)
        assert label(game, "hud:gun") == "SNIPER", label(game, "hud:gun")
        cap = max_zoom(rect("playfield"))
        r.frame(30)
        base = camera_scale(label, game)
        assert label(game, "zoom:max") == "/4x", label(game, "zoom:max")
        for level in (1, 2, 3, 4, 1):
            assert label(game, "zoom:level") == f"{level}x", (level, label(game, "zoom:level"))
            r.frame(30)
            got = camera_scale(label, game)
            want = base / min(level, cap)
            assert abs(got / want - 1) < 0.03, (tag, level, got, want, cap)
            soldier = 52 * got
            assert soldier >= 29.5, (tag, level, soldier)
            if level in (1, 2, 4):
                r.save_screenshot(game, str(ROOT / f"shots/jetpack-zoom-{tag}-sniper-{level}x.png"))
            tap("button:zoom")
            r.frame(2)
        assert not r.read_console(game), r.read_console(game)
        print(f"zoom {tag}: sniper 1/2/3/4x and wrap ok (cap {cap:.2f}x)")


def run_rifle(size, part):
    """The rifle (pistol on the back, the default loadout). Part "steps": the zoom button, Z and the wheel step and wrap.
    Part "swap": a gun change puts the zoom back to 1x. A fresh session each, right after landing: bots would kill a slow player."""
    tag = f"{size[0]}x{size[1]}"
    with Session(shell_binary=shell_binary(), offscreen=size) as s:
        r = s.rpc
        r.signup("maker", "jetpack zoom passphrase")
        ws = r.create_workspace(f"Jetpack zoom {part} " + tag)
        app, rect, has, tap, label = helpers(r, ws)
        game = app("Jetpack Arena", "jetpack")
        tap("map:pick:yard")
        tap("match:start")
        r.frame(100)
        assert label(game, "hud:gun") == "RIFLE" and label(game, "hud:other") == "Q  PISTOL 12", (label(game, "hud:gun"), label(game, "hud:other"))

        def settle(frames=18):
            r.frame(frames)  # the zoom eases in about a quarter second
            return camera_scale(label, game)

        def zoom_to(level, scale):
            assert label(game, "zoom:level") == f"{level:g}x", (level, label(game, "zoom:level"))
            got = settle()
            assert abs(got * level / scale - 1) < 0.03, (level, got, scale, label(game, "hud:health"))
            assert (label(game, "hud:zoom") == f"{level:g}x") == (level > 1), label(game, "hud:zoom")

        s1 = settle()
        zoom_to(1, s1)
        if part == "steps":
            zoom_btn = rect("button:zoom")
            for stick in ("control:move", "control:aim", "button:reload", "button:swap"):
                assert not overlap(zoom_btn, rect(stick)), stick
            assert label(game, "zoom:max") == "/2x", label(game, "zoom:max")
            shot_zoom = lambda name: r.save_screenshot(game, str(ROOT / f"shots/jetpack-zoom-{tag}-{name}.png"))
            shot_zoom("rifle-1x")
            tap("button:zoom")
            zoom_to(1.5, s1)
            tap("button:zoom")
            zoom_to(2, s1)
            shot_zoom("rifle-2x")
            tap("button:zoom")
            zoom_to(1, s1)  # wrapped
            r.keyboard("KeyZ", "z", True)
            r.keyboard("KeyZ", "z", False)
            r.frame(2)
            zoom_to(1.5, s1)  # Z steps too
            play = rect("playfield")
            cx, cy = play["x"] + play["w"] / 2, play["y"] + play["h"] / 3
            r.wheel(cx, cy, 0, 30)
            r.frame(2)
            zoom_to(2, s1)  # wheel out widens
            r.wheel(cx, cy, 0, 30)
            r.frame(2)
            zoom_to(2, s1)  # and stops at the gun's widest level
            r.wheel(cx, cy, 0, -30)
            r.frame(2)
            zoom_to(1.5, s1)  # wheel in closes
        else:
            tap("button:zoom")
            zoom_to(1.5, s1)
            r.frame(10)
            tap("button:swap")  # a gun change: back to 1x on the pistol, whose scope is shorter
            assert label(game, "hud:gun") == "PISTOL", label(game, "hud:gun")
            zoom_to(1, s1)
            assert label(game, "zoom:max") == "/1.25x", label(game, "zoom:max")
            tap("button:zoom")
            zoom_to(1.25, s1)
            r.frame(10)
            tap("button:swap")  # back to the rifle: 1x again, though the pistol was left at 1.25x
            assert label(game, "hud:gun") == "RIFLE", label(game, "hud:gun")
            zoom_to(1, s1)
        assert int(label(game, "hud:health").split()[-1]) > 0, "the player died mid-flow: " + label(game, "hud:health")
        assert not r.read_console(game), r.read_console(game)
        print(f"zoom {tag}: rifle {part} ok")


if __name__ == "__main__":
    (ROOT / "shots").mkdir(exist_ok=True)
    edited = run_editor()
    run_seeded(edited, (960, 540))
    run_seeded(edited, (780, 440))
    run_sniper((960, 540))
    run_sniper((780, 440))
    for size in ((960, 540), (780, 440)):
        run_rifle(size, "steps")
        run_rifle(size, "swap")
    print("jetpack pickups smoke ok")
