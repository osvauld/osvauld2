"""Single-player jetpack arena: real Lua, captured fingers, dual-stick flight, the camera, maps, combat and pause."""
import base64
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from osvauld.session import Session, shell_binary
from osvauld.png import Image

ROOT = Path(__file__).resolve().parent.parent
BIG = ("outpost", "catacombs", "rooftops")
NAMES = {"outpost": "Cliffside Outpost", "catacombs": "The Catacombs", "rooftops": "Ruined Rooftops", "yard": "Training Yard"}
# Mirrors sticks.lua: the stick's travel radius is 0.42 of its box.
RING = 0.42


def find(tree, identity):
    if tree.get("id") == identity:
        return tree
    for child in tree.get("children", []):
        found = find(child, identity)
        if found:
            return found


def overlap(a, b):
    return a["x"] < b["x"] + b["w"] and b["x"] < a["x"] + a["w"] and a["y"] < b["y"] + b["h"] and b["y"] < a["y"] + a["h"]


def run(size):
    with Session(shell_binary=shell_binary(), offscreen=size) as s:
        r = s.rpc
        r.signup("pilot", "jetpack smoke passphrase")
        ws = r.create_workspace("Jetpack smoke")
        item = r.create_item(ws["id"], "Jetpack Arena", "app")["id"]
        r.upload_folder(item, ROOT / "demo_apps/jetpack")
        r.open_item(item)
        r.frame(2)

        def label(identity):
            node = find(r.dump_tree(item), identity)
            assert node, (identity, r.read_console(item))
            return node["text"]

        def number(identity):
            return int(label(identity).split()[-1])

        def rect(identity):
            return next(e for e in r.rects() if e["id"] == identity)

        def tap(identity):
            r.click_at(*r.centre_of(identity))
            r.frame(2)

        def camera():
            """The camera as the game publishes it: world x, y at the playfield's top-left, px per unit."""
            x, y, scale = (float(v) for v in label("camera:view").split())
            return x, y, scale

        def player_box():
            return next((e for e in r.rects() if e["id"] == "player"), None)

        def on_field(box, field):
            cx, cy = box["x"] + box["w"] / 2, box["y"] + box["h"] / 2
            return field["x"] <= cx <= field["x"] + field["w"] and field["y"] <= cy <= field["y"] + field["h"]

        def check_clear(where):
            """The soldier (if alive) is on the playfield and under no control."""
            box, field = player_box(), rect("playfield")
            if box is None:
                return False
            assert on_field(box, field), (size, where, "player left the playfield", box, field)
            for identity in ("control:move", "control:aim", "button:reload"):
                assert not overlap(box, rect(identity)), (size, where, "player is under", identity, box, rect(identity))
            return True

        def pick(map_id):
            """Choose an arena on the start (or pause) overlay; the match is Ready on it."""
            tap(f"map:pick:{map_id}")
            assert label("match:state") == "Ready" and label("map:name") == NAMES[map_id], (map_id, label("map:name"))

        def stick(identity):
            """Centre of a stick and its travel radius in px."""
            box = rect(identity)
            return box["x"] + box["w"] / 2, box["y"] + box["h"] / 2, box["w"] * RING

        assert not r.read_console(item), r.read_console(item)
        assert label("match:state") == "Ready"
        # The start overlay offers every arena; the first (a big one) is chosen.
        for map_id in (*BIG, "yard"):
            assert any(e["id"] == f"map:pick:{map_id}" for e in r.rects()), (size, map_id)
        assert label("map:name") == NAMES["outpost"], label("map:name")
        r.save_screenshot(item, str(ROOT / "shots" / f"jetpack-landscape-start-{size[0]}.png"))
        normal_height = rect("playfield")["h"]
        tap("shell:fullscreen")
        assert not any(e["id"] == "tab:home" for e in r.rects())
        assert rect("playfield")["h"] > normal_height, "fullscreen must return shell chrome space"
        tap("shell:exit-fullscreen")
        assert any(e["id"] == "tab:home" for e in r.rects())
        assert abs(rect("playfield")["h"] - normal_height) < 1

        # --- Controls, soldier size and the framing, on every big map.
        for map_id in BIG:
            pick(map_id)
            tap("match:start")
            assert label("match:state") == "Playing"
            assert number("hud:bots") == 4
            for identity in ("control:move", "control:aim", "match:pause", "button:reload"):
                e = rect(identity)
                assert e["w"] >= 40 and e["h"] >= 36, (size, identity, e)
                assert 0 <= e["x"] < size[0] and 0 <= e["y"] < size[1], (size, e)
            assert not any(e["id"] == "control:boost" for e in r.rects()), "the JET button is gone"
            assert not overlap(rect("control:move"), rect("control:aim"))
            assert not overlap(rect("button:reload"), rect("control:aim")) and not overlap(rect("button:reload"), rect("control:move"))
            assert check_clear(f"{map_id} spawn"), "the player is not on screen at spawn"
            cam_x, cam_y, scale = camera()
            field = rect("playfield")
            # A soldier is ~110-140 px tall on a 960x540 view (~90 on a 780x360 landscape phone).
            assert field["w"] > field["h"], (size, "the game is landscape-only", field)
            assert 85 <= 52 * scale <= 150, (size, map_id, "soldier px", 52 * scale)
            assert field["w"] / scale * 3 < {"outpost": 4400, "catacombs": 4000, "rooftops": 4800}[map_id], \
                "the map is much larger than the view"

            # Dual sticks at once: left pushed up-right at the hover height, right pushed past its ring.
            mx, my, mring = stick("control:move")
            ax, ay, aring = stick("control:aim")
            r.touch(1, "start", mx, my)
            r.touch(1, "move", mx + 0.5 * mring, my - 0.9 * mring)
            r.touch(2, "start", ax, ay)
            r.touch(2, "move", ax + 0.85 * aring, ay - 0.2 * aring)
            r.frame(40)
            assert number("hud:fuel") < 100, "left stick up did not fly"
            assert number("hud:altitude") > 60, number("hud:altitude")  # standing is ~30 above the floor
            assert number("hud:fired") > 0, "right stick past the ring did not fire"
            x1, _, _ = camera()
            assert x1 > cam_x or map_id == "x", (map_id, "camera did not follow the walk right", cam_x, x1)
            assert check_clear(f"{map_id} two sticks")
            r.save_screenshot(item, str(ROOT / "shots" / f"jetpack-camera-{map_id}-air-{size[0]}.png"))
            r.touch(1, "end", mx, my)
            r.touch(2, "end", ax, ay)

            # The scripted flight: up and right, back down and left to the wall, up the wall, down to the
            # floor. Every few frames the soldier must be on screen, in the band and under no control.
            legs = [(0.7, -1.0, 150), (0.8, 0.0, 120), (-0.9, 0.3, 240), (-0.9, -1.0, 150), (-0.2, 0.6, 240), (-0.9, 0.5, 150)]
            for index, (dx, dy, frames) in enumerate(legs):
                r.touch(3, "start", mx, my)
                r.touch(3, "move", mx + dx * mring, my + dy * mring)
                for step in range(frames // 10):
                    r.frame(10)
                    check_clear(f"{map_id} flight leg {index} step {step}")
                r.touch(3, "end", mx, my)
            r.frame(30)
            check_clear(f"{map_id} landed")
            if number("hud:altitude") < 40:  # standing on the floor: the view stops at the map's bottom
                box, field = player_box(), rect("playfield")
                assert box["y"] + box["h"] > field["y"] + 0.8 * field["h"], (size, map_id, "floor soldier is high on screen", box, field)
            shot = ROOT / "shots" / f"jetpack-camera-{map_id}-{size[0]}.png"
            r.save_screenshot(item, str(shot))
            assert not r.read_console(item), r.read_console(item)
            # Back to the start screen from the pause overlay to choose the next arena.
            tap("match:pause")
            assert label("match:state") == "Paused"

        # --- Reload: the R-slot button, mid-magazine, starts a reload; R does too.
        pick("yard")
        tap("match:start")
        assert label("hud:ammo").startswith("30/"), label("hud:ammo")
        assert find(r.dump_tree(item), "hud:reload") is None
        ax, ay, aring = stick("control:aim")
        r.touch(2, "start", ax, ay)
        r.touch(2, "move", ax + 0.85 * aring, ay - 0.2 * aring)
        r.frame(12)
        r.touch(2, "end", ax, ay)
        assert 0 < int(label("hud:ammo").split("/")[0]) < 30, label("hud:ammo")
        assert find(r.dump_tree(item), "hud:reload") is None
        tap("button:reload")
        assert label("hud:reload") == "RELOADING", "the reload button did nothing"
        r.frame(240)
        assert find(r.dump_tree(item), "hud:reload") is None and label("hud:ammo").startswith("30/"), label("hud:ammo")
        r.keyboard("KeyF", "f", True)
        r.frame(10)
        r.keyboard("KeyF", "f", False)
        r.keyboard("KeyR", "r", True)
        r.keyboard("KeyR", "r", False)
        r.frame(2)
        assert label("hud:reload") == "RELOADING", "R did not reload"
        assert not r.read_console(item), r.read_console(item)
        tap("match:pause")

        # --- Proportional thrust: stick height sets the throttle (hovering is a mid-stick, full is a climb).
        pick("yard")
        gains = {}
        for name, height in (("low", 0.3), ("mid", 0.75), ("full", 1.0)):
            tap("match:start") if label("match:state") == "Ready" else tap("match:restart")
            base = number("hud:altitude")
            mx, my, mring = stick("control:move")
            r.touch(1, "start", mx, my)
            r.touch(1, "move", mx, my - height * mring)
            r.frame(30)
            gains[name] = number("hud:altitude") - base
            r.touch(1, "end", mx, my)
        assert gains["low"] <= 2, ("a low push must not lift off", gains)
        assert 5 < gains["mid"] < gains["full"], ("thrust follows stick height", gains)
        assert gains["full"] > 1.4 * gains["mid"], ("thrust follows stick height", gains)

        # Fire horizontally at the bunker. Read actual pixels on either side of its wall (world coords
        # from the camera probe): walk up to ~250, then shoot level at the cover at x 456..504.
        tap("match:restart")
        r.keyboard("KeyD", "d", True)
        r.frame(30)
        r.keyboard("KeyD", "d", False)
        r.frame(20)
        field = rect("playfield")
        cam_x, cam_y, scale = camera()
        p = player_box()
        player_x = cam_x + (p["x"] + p["w"] / 2 - field["x"]) / scale

        def px(wx):
            return field["x"] + (wx - cam_x) * scale

        def py(wy):
            return field["y"] + (wy - cam_y) * scale

        r.move_to(px(player_x + 140), py(427))
        r.keyboard("KeyF", "f", True)
        r.frame(42)
        r.keyboard("KeyF", "f", False)
        shot = r.screenshot(item)
        Path("/tmp/jetpack-cover.png").write_bytes(base64.b64decode(shot["png_base64"]))
        pixels = Image(base64.b64decode(shot["png_base64"]))

        def tracers(lo, hi):
            box = (round(px(lo)), round(py(423)), round(px(hi)), round(py(431)))
            return sum(red > 230 and green > 200 and blue < 190
                       for y in range(box[1], box[3]) for x in range(box[0], box[2])
                       for red, green, blue in [pixels.at(x, y)])

        assert tracers(player_x + 25, 440) > 0, "control case: no player tracers before cover"
        assert tracers(525, cam_x + field["w"] / scale - 2) == 0, "player shots passed through the bunker"
        r.keyboard("KeyD", "d", True)
        r.frame(120)
        r.keyboard("KeyD", "d", False)
        cam_x, cam_y, scale = camera()
        p = player_box()
        player_x = cam_x + (p["x"] + p["w"] / 2 - field["x"]) / scale
        assert abs(player_x - 444) < 2, ("player did not stop at cover", player_x)
        tap("match:restart")

        # Stationary fingers are held while frames advance; cancelling aim does not release the walk/thrust finger.
        mx, my, mring = stick("control:move")
        ax, ay, aring = stick("control:aim")
        r.touch(1, "start", mx, my)
        r.touch(1, "move", mx + 0.6 * mring, my - 0.9 * mring)
        r.touch(2, "start", ax, ay)
        r.touch(2, "move", ax + 0.9 * aring, ay - 0.1 * aring)
        r.frame(24)
        assert number("hud:fuel") < 100
        assert number("hud:altitude") > 60  # 30 is standing on the ground
        assert number("hud:fired") > 0
        # A custom viewport is a temporary observation, not a control release or simulation tick.
        before_capture = number("hud:fired")
        fuel_before_capture = number("hud:fuel")
        r.screenshot(item, width=640, height=360)
        assert number("hud:fired") == before_capture
        assert number("hud:fuel") == fuel_before_capture
        r.frame(12)
        assert number("hud:fired") > before_capture, "capture released a held aiming finger"
        fired = number("hud:fired")
        r.touch(2, "cancel", ax + 30, ay - 6)
        r.frame(12)
        assert number("hud:fired") == fired
        assert number("hud:fuel") < 90
        r.touch(1, "end", mx, my)

        tap("match:pause")
        assert label("match:state") == "Paused"
        frozen = r.dump_tree(item)
        r.frame(100)
        assert r.dump_tree(item) == frozen, "paused match moved"
        r.save_screenshot(item, str(ROOT / "shots" / f"jetpack-landscape-paused-{size[0]}.png"))
        tap("match:resume")
        fired = number("hud:fired")
        r.frame(20)
        assert number("hud:fired") == fired, "resume retained held fire"

        # An idle player is attacked, dies and respawns; no test-only gameplay commands.
        tap("match:restart")
        for _ in range(45):
            r.frame(60)
            if number("hud:deaths") > 0:
                break
        assert number("hud:deaths") > 0, "bots could not defeat an idle player"
        r.frame(150)
        assert label("match:state") == "Playing"
        assert number("hud:health") > 0, "player did not respawn"

        # Aim at bots using their rendered, accessible hit geometry.
        tap("match:restart")
        r.keyboard("Space", " ", True)
        r.keyboard("KeyF", "f", True)
        for _ in range(240):
            enemies = [e for e in r.rects() if e["id"].startswith("bot:")]
            if enemies:
                target = enemies[0]
                r.move_to(target["x"] + target["w"] / 2, target["y"] + target["h"] / 2)
            r.frame(6)
            if number("hud:kills") > 0:
                break
        r.keyboard("Space", " ", False)
        r.keyboard("KeyF", "f", False)
        assert number("hud:kills") > 0, "aimed shots did not kill a bot"
        r.frame(240)
        assert number("hud:bots") == 2, "defeated bot did not respawn"
        assert not [k for k in r.read_data(item) if not k.startswith("asset:")], "match state leaked into documents"
        assert not r.read_console(item), r.read_console(item)
        r.save_screenshot(item, str(ROOT / "shots" / f"jetpack-landscape-playing-{size[0]}.png"))
        before = r.dump_tree(item)
        for _ in range(3):
            r.screenshot(item)
            assert r.dump_tree(item) == before, "capture or inspection advanced the match"
        # A fourth finger can pause while the gameplay fingers are still held.
        mx, my, mring = stick("control:move")
        px_, py_ = r.centre_of("match:pause")
        r.touch(8, "start", mx, my)
        r.touch(8, "move", mx + 30, my)
        r.touch(9, "start", px_, py_)
        r.touch(9, "end", px_, py_)
        assert label("match:state") == "Paused", "held joystick prevented touch-operated pause"
        r.touch(8, "cancel", mx + 30, my)
        tap("match:restart")
        start_x, _, _ = camera()
        assert start_x < 135, ("camera starts left of the yard spawn", start_x)
        for code, key in (("KeyD", "d"), ("Space", " "), ("KeyF", "f")):
            r.keyboard(code, key, True)
        r.frame(270)
        for code, key in (("KeyD", "d"), ("Space", " "), ("KeyF", "f")):
            r.keyboard(code, key, False)
        travelled_x, _, _ = camera()
        assert travelled_x > start_x + 400, ("camera did not follow travel", start_x, travelled_x)
        check_clear("keyboard travel")
        r.save_screenshot(item, f"/tmp/jetpack-camera-{size[0]}.png")
        # Closing a fullscreen tab must not transfer immersive mode to the previous app.
        second = r.create_item(ws["id"], "Second Arena", "app")["id"]
        r.upload_folder(second, ROOT / "demo_apps/jetpack")
        r.open_item(second)
        tap("shell:fullscreen")
        r.request("CloseItem", item_id=second)
        r.frame(2)
        assert any(e["id"] == "tab:home" for e in r.rects())
        assert not any(e["id"] == "shell:exit-fullscreen" for e in r.rects())
        print(f"jetpack {size}: arenas, dual sticks, framing, proportional thrust, combat, respawn, pause and capture passed")


def portrait_rows():
    """Landscape-only: a playfield taller than wide shows the rotate card, no controls, and the match clock stands still."""
    shots = ROOT / "shots"
    shots.mkdir(exist_ok=True)

    def ids(r):
        return {e["id"] for e in r.rects()}

    def no_controls(r, where):
        found = [i for i in ids(r) if i.startswith(("button:", "control:", "match:", "map:pick"))]
        assert not found, (where, "controls under the rotate card", found)

    # A phone upright from the start: the card, no way to deploy (Enter included), and nothing runs.
    with Session(shell_binary=shell_binary(), offscreen=(393, 780)) as s:
        r = s.rpc
        r.signup("pilot", "jetpack smoke passphrase")
        ws = r.create_workspace("Jetpack portrait")
        item = r.create_item(ws["id"], "Jetpack Arena", "app")["id"]
        r.upload_folder(item, ROOT / "demo_apps/jetpack")
        r.open_item(item)
        r.frame(3)
        field = next(e for e in r.rects() if e["id"] == "playfield")
        card = next((e for e in r.rects() if e["id"] == "rotate:card"), None)
        assert field["h"] > field["w"] and card, ("expected the rotate card on a portrait playfield", field, sorted(ids(r)))
        assert card["w"] >= field["w"] - 1 and card["h"] >= field["h"] - 1, ("the card fills the playfield", card, field)
        no_controls(r, "393x780")
        clock = find(r.dump_tree(item), "rotate:clock")["text"]
        assert clock == "Ready 0.000", clock
        r.keyboard("Enter", "Enter", True)
        r.keyboard("Enter", "Enter", False)
        r.frame(120)
        assert find(r.dump_tree(item), "rotate:clock")["text"] == clock, "the match clock ran under the rotate card"
        assert not r.read_console(item), r.read_console(item)
        r.save_screenshot(item, str(shots / "jetpack-landscape-rotate-393x780.png"))

    # Playing in a landscape window, then the playfield turns portrait (fullscreen drops the shell's top chrome, so
    # 400 px wide and 460 tall is landscape inside the shell and portrait in fullscreen): the match pauses, the card
    # shows, the clock stands still; back to landscape the pause overlay shows and only a tap resumes.
    with Session(shell_binary=shell_binary(), offscreen=(400, 460)) as s:
        r = s.rpc
        r.signup("pilot", "jetpack smoke passphrase")
        ws = r.create_workspace("Jetpack rotate")
        item = r.create_item(ws["id"], "Jetpack Arena", "app")["id"]
        r.upload_folder(item, ROOT / "demo_apps/jetpack")
        r.open_item(item)
        r.frame(3)

        def tap(identity):
            r.click_at(*r.centre_of(identity))
            r.frame(3)

        def text(identity):
            node = find(r.dump_tree(item), identity)
            return node["text"] if node else None

        field = next(e for e in r.rects() if e["id"] == "playfield")
        assert field["w"] >= field["h"], ("the 400x460 window must be landscape inside the shell", field)
        tap("match:start")
        r.frame(60)
        assert text("match:state") == "Playing"
        tap("shell:fullscreen")
        field = next(e for e in r.rects() if e["id"] == "playfield")
        assert field["h"] > field["w"], ("fullscreen should turn the playfield portrait", field)
        assert "rotate:card" in ids(r)
        no_controls(r, "rotated while playing")
        clock = text("rotate:clock")
        assert clock.startswith("Paused "), clock
        r.frame(120)
        assert text("rotate:clock") == clock, "the match clock ran under the rotate card"
        r.save_screenshot(item, str(shots / "jetpack-landscape-rotate-playing.png"))
        tap("shell:exit-fullscreen")
        assert "rotate:card" not in ids(r), "the card must go when the phone is landscape again"
        assert text("match:state") == "Paused", "rotating back must not resume the match"
        assert "match:resume" in ids(r)
        r.frame(60)
        assert text("match:state") == "Paused"
        r.save_screenshot(item, str(shots / "jetpack-landscape-paused-400x460.png"))
        tap("match:resume")
        assert text("match:state") == "Playing"
        assert not r.read_console(item), r.read_console(item)
    print("jetpack portrait: rotate card, no controls, clock stands still, pause overlay on rotating back passed")


if __name__ == "__main__":
    run((960, 540))
    run((780, 440))  # a landscape phone: the playfield is ~780x360
    portrait_rows()
