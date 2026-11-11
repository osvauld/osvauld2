"""Brawl, steps 1-2, through the shell (plan: docs/design/brawl-movement.md §0). S-1: upload and
open, the console is clean, the fighter stands on the ground with 3 stocks, a screenshot. Ticks per
driver op are counted and printed: `rpc.frame(1)` is 2 ticks, now and then 1 (the match's float
carry falls a hair short, and it stays a tick behind after); `rpc.keyboard` is itself a driven
frame plus 8 ms, so 2 or 3. S-3: Space held 40 frames peaks at exactly 331.5; a tap peaks between
107.25 and 331.5. S-2: run, release, reverse (a skid, then facing flips). S-4: a jump from under
the left platform ends standing on it; ArrowDown drops through to the ground. S-6: the double
jump, fast fall, ground and air dash and the wall jump each change their readout. S-5: off the
stage edge, steered back to the wall and wall jumped, a stock is lost, and the fighter is back at
the respawn point 120 ticks (1 s) after it went out. S-9: the hidden `script` button runs the
1 200-tick timeline twice; both runs give the same checkpoint hashes and the same final state,
which is the one scripts/test_brawl_moves.lua prints for P-M12. The state is read from the hidden
`fighter:state` readout; facts here don't depend on which tick a key lands on. Step 3, S-7: the stage's
pictures are built a few a frame (`art:loaded` n/n); the dump lists them by path and bytes; across
S-1-S-5 the fighter's frame stays inside the playfield and the `camera:view` zoom inside its limits;
screenshots step3_idle/jump/edge.png go to shots/brawl/. Step 4, S-8: standing, running, in the air
and facing left, the robot (`fighter:robot`) is drawn where `fighter:state` says, feet on the box's
bottom centre, on the clip its act names at the frame `act_t` gives (`look:state`); facing left, the
parts under a row of points across it are the mirror of those facing right (`look:hover`), so the
parts swap sides. Screenshots step4_idle/run/air/mirrored.png go to shots/brawl/. Step 5: the acts
play the robot's own clips (`land` after a jump with no key held, its boots on the floor by pixels;
`jump2`; `wall`, facing the wall), and `look:state`'s `blend` is set only in an act's first
blend_ticks, at act_t / blend_ticks. Screenshots step5_land/jump2/wall.png."""

import shutil
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from osvauld.png import Image
from osvauld.session import Session, shell_binary

SCREEN = (900, 700)
ROOT = Path(__file__).resolve().parent.parent
SHOTS = ROOT / "shots" / "brawl"
# demo_apps/brawl/maps/yard.lua and tune.lua
LEFT_TOP, LEFT_X = -260, (-620, -300)
TOP_TOP = -520
RUN_SPEED, FAST_FALL, WALL_SLIDE = 7, 20, 3
GROUND_EDGE = 900
RESPAWN = (0, -700)
RESPAWN_TICKS = 120
# demo_apps/brawl/maps/yard.lua: the camera's bounds, the body, and the art the dump must list
CAMERA = (-1500, -1100, 3000, 1800)
BODY = (60, 150)
# (maps step 4: the tilemap block's top, caps and walls, the fog and the light hills; the clouds are SVG)
ART = {"art/background_mountain_1.png", "art/background_mountain_2.png", "art/background_fog_2.png",
       "art/foreground_basic_1.png", "art/foreground_basic_3.png", "art/ground_block_top.png", "art/ground_cap_l.png",
       "art/ground_cap_r.png", "art/ground_wall_l.png", "art/ground_wall_r.png", "art/tree.png", "art/tree_2.png",
       "art/bush.png", "art/rock.png", "art/grass_3.png", "art/platform_4.png"}
# The drawn grass against the soles (maps step 2): a ledge's lift comes from the asset library (10 rows of
# transparent pixels above the planks, platforms.lua), so the soles sit on the drawn grass.
SURFACE_GAP_MAX = 1


SCRIPT_TICKS = 1200

# demo_apps/brawl/look.lua: the clip each act plays, and each clip's (length, loop) (step 5: the
# robot's own, characters/robot/clips/, beside Paka's idle and run).
CLIP_OF = {"idle": "idle", "run": "run", "turn": "turn", "dash": "dash", "rise": "rise", "jump2": "jump2",
           "apex": "apex", "fall": "fall", "wall": "wall", "land": "land"}
CLIPS = {"idle": (2.0, True), "run": (0.8, True), "rise": (0.3, False), "apex": (0.15, False), "fall": (0.6, True),
         "land": (0.1, False), "jump2": (0.4, False), "dash": (0.2, True), "wall": (0.5, True), "turn": (0.1, False)}
BLEND_TICKS = 4
# the robot drawing's box and its root pivot (the feet): characters/paka/rig.lua
ROBOT_SIZE, ROBOT_FEET = (396, 239), (198, 239)
SCALE = 0.75  # Look.SCALE: the robot is drawn at this fraction of its drawing


def clip_index(clip, act_t):
    """look.lua's Look.at: the frame index at 60 a second, wrapped for a loop, held for a once clip."""
    length, loop = CLIPS[clip]
    i = int(act_t) // 2
    if loop:
        return i % round(length * 60)
    return min(i, int(length * 60 + 1e-9) + 1)


def lua_final():
    """The final state line P-M12's straight run prints, or None without a Lua."""
    lua = shutil.which("lua5.4") or shutil.which("lua")
    if not lua:
        return None
    out = subprocess.run([lua, "scripts/test_brawl_moves.lua"], cwd=ROOT, capture_output=True, text=True, check=True).stdout
    for line in out.splitlines():
        if line.startswith("P-M12 final: "):
            return line[len("P-M12 final: "):]
    raise AssertionError(out)


def find(tree, identity):
    if isinstance(tree, dict):
        if tree.get("id") == identity:
            return tree
        tree = tree.get("children", [])
    for child in tree:
        found = find(child, identity)
        if found:
            return found


def main():
    SHOTS.mkdir(parents=True, exist_ok=True)
    with Session(shell_binary=shell_binary(), offscreen=(900, 700)) as s:
        r = s.rpc
        r.signup("brawler", "brawl smoke passphrase")
        ws = r.create_workspace("Brawl smoke")
        item = r.create_item(ws["id"], "Brawl", "app")["id"]
        r.upload_folder(item, ROOT / "demo_apps/brawl")
        r.open_item(item)
        r.frame(2)
        # Fighting step 1: the app opens against the training dummy; this smoke is the movement
        # plan's, one fighter alone (brawl-fighting.md S-F8).
        r.click(item, "vs:none")
        r.frame(2)

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

        def readout(name):
            node = find(r.dump_tree(item), name)
            assert node, (name, r.read_console(item))
            return {k: (v if k == "act" else float(v)) for k, v in (p.split("=") for p in node["text"].split())}

        view_checks = [0]

        def check_view(st):
            """S-7: the fighter's frame is inside the playfield (the `script` button sits at its top-left
            corner) and the camera's zoom inside its limits. Skipped while it is out of play or below the
            camera's bounds, where the view is rightly elsewhere."""
            cam = readout("camera:view")
            assert cam["zoom_min"] - 1e-9 <= cam["zoom"] <= cam["zoom_max"] + 1e-9, cam
            assert CAMERA[0] - 1e-9 <= cam["x"] and cam["x"] + cam["w"] <= CAMERA[0] + CAMERA[2] + 1e-9, cam
            assert CAMERA[1] - 1e-9 <= cam["y"] and cam["y"] + cam["h"] <= CAMERA[1] + CAMERA[3] + 1e-9, cam
            if st["act"] in ("out", "ko") or st["y"] > CAMERA[1] + CAMERA[3] or abs(st["x"]) > CAMERA[2] / 2 - BODY[0]:
                return
            rects = {x["id"]: x for x in r.rects()}
            if "fighter" not in rects:
                return
            f, corner = rects["fighter"], rects["script"]
            eps = 1.0
            assert corner["x"] - eps <= f["x"] and f["x"] + f["w"] <= corner["x"] + SCREEN[0] + eps, (f, corner, st)
            assert corner["y"] - eps <= f["y"] and f["y"] + f["h"] <= corner["y"] + SCREEN[1] - 38 + eps, (f, corner, st)
            view_checks[0] += 1

        def robot_check(label, shot=None):
            """S-8: the robot is drawn where the state says, on the clip its act names, mirrored by face."""
            tree = r.dump_tree(item)

            def parse(name):
                node = find(tree, name)
                assert node, (name, r.read_console(item))
                return {k: (v if k in ("act", "robot", "clip") else float(v)) for k, v in (p.split("=") for p in node["text"].split())}
            st, ls, cam = parse("fighter:state"), parse("look:state"), parse("camera:view")
            assert ls["robot"] == "scout" and ls["face"] == st["face"], (label, st, ls)
            assert ls["clip"] == CLIP_OF[st["act"]], (label, st["act"], ls)
            assert ls["index"] == clip_index(ls["clip"], st["act_t"]), (label, st["act_t"], ls)
            # a blend only in the act's first blend_ticks, by act_t / blend_ticks (P-A2)
            assert ls["blend"] == -1 or (st["act_t"] < BLEND_TICKS and ls["blend"] == st["act_t"] / BLEND_TICKS), (label, st, ls)
            # drawn on by match.ahead, under one tick
            assert abs(ls["x"] - st["x"]) <= abs(st["vx"]) + 1e-9 and abs(ls["y"] - st["y"]) <= abs(st["vy"]) + 1e-9, (label, st, ls)
            rects = {x["id"]: x for x in r.rects()}
            robot, box, corner, k = rects["fighter:robot"], rects["fighter"], rects["script"], ls["k"]
            want = (corner["x"] + (ls["x"] - ROBOT_FEET[0] * SCALE - cam["x"]) * k, corner["y"] + (ls["y"] - ls["feet"] * SCALE - cam["y"]) * k,
                    ROBOT_SIZE[0] * SCALE * k, ROBOT_SIZE[1] * SCALE * k)
            got = (robot["x"], robot["y"], robot["w"], robot["h"])
            assert all(abs(a - b) <= 1.0 for a, b in zip(got, want)), (label, got, want)
            assert abs(k - SCREEN[0] / cam["w"]) < 1e-9, (label, k, cam)
            # The feet line is the box's bottom centre: the boots' lowest point standing and running
            # (look.lua plants it), the root's pivot in the air.
            planted = st["act"] in ("idle", "run", "turn", "dash", "land")
            assert ls["feet"] == (ls["sole"] if planted else ROBOT_FEET[1]), (label, ls)
            feet = (robot["x"] + ROBOT_FEET[0] * SCALE * k, robot["y"] + ls["feet"] * SCALE * k)
            floor = box["y"] + box["h"]
            assert abs(feet[0] - (box["x"] + box["w"] / 2)) <= 1.0 and abs(feet[1] - floor) <= 1.0, (label, feet, box)
            gap = None
            if shot:
                path = SHOTS / shot
                r.save_screenshot(item, str(path))
                if planted:
                    # By pixels: the lowest row of the boots' dark soles (ink outline, dark trim; not
                    # grass green, not the ground's purple), followed down from inside the boots
                    # until a row has none, is within 2 px of the floor line.
                    img = Image(path.read_bytes())
                    assert img.width == SCREEN[0], img.width
                    cols = [int(robot["x"] + u * SCALE * k) for u in range(150, 250, 2)]

                    def dark(x, y):
                        rr, g, b = img.at(x, y)
                        return max(rr, g, b) < 80 and g < rr + 25
                    y, lowest = int(floor - 6), None
                    while y < floor + 12:
                        if any(dark(x, y) for x in cols):
                            lowest = y
                        elif lowest is not None:
                            break
                        y += 1
                    assert lowest is not None, (label, "no sole pixels near the floor line", floor)
                    gap = floor - (lowest + 1)
                    assert abs(gap) <= 2, (label, "feet are off the floor by", gap, "px", lowest, floor)
                    # And against the drawn surface: the first grass-green row under the feet line.
                    def green(x, y):
                        rr, g, b = img.at(x, y)
                        return g > rr + 40 and g > b + 40
                    top = next((y for y in range(int(floor) - 8, int(floor) + 20) if sum(green(x, y) for x in cols) > len(cols) // 2), None)
                    surface_gap = None if top is None else top - (lowest + 1)
                    print(f"S-8 {label}: drawn surface top at {top}, {surface_gap} px below the soles")
                    assert surface_gap is not None and abs(surface_gap) <= SURFACE_GAP_MAX, (label, "feet off the drawn surface by", surface_gap)
            print(f"S-8 {label}: act={st['act']} clip={ls['clip']} index={int(ls['index'])} face={int(ls['face'])} "
                  f"frame={tuple(round(v, 1) for v in got)} sole={ls['sole']:.2f} pixel gap={gap}")
            return st, ls

        def hover_row(rows=(75, 150), step=8):
            """The part under each of a row of points across the robot's box, by drawing x: {(u, v): part}."""
            out = {}
            # The camera eases on render time, so the robot drifts on screen until it settles; a
            # pointer move is itself 8 ms of it.
            prev = None
            for _ in range(240):
                frames(1)
                cam = readout("camera:view")
                if prev and all(abs(cam[c] - prev[c]) < 0.01 for c in ("x", "y", "w")):
                    break
                prev = cam
            for v in rows:
                robot = {x["id"]: x for x in r.rects()}["fighter:robot"]
                k = robot["w"] / ROBOT_SIZE[0]  # already includes SCALE
                for u in range(102, 295, step):
                    r.move_to(robot["x"] + u * k, robot["y"] + v * k)
                    part = find(r.dump_tree(item), "look:hover")["text"]
                    out[(u, v)] = part.split("/", 1)[1] if part.startswith("fighter:robot/") else None
            r.move_to(5, SCREEN[1] - 5)
            last[0] = state()["tick"]  # pointer moves are time too; don't count them as a key's ticks
            return out

        def state():
            node = find(r.dump_tree(item), "fighter:state")
            assert node, r.read_console(item)
            out = {}
            for pair in node["text"].split():
                k, v = pair.split("=")
                out[k] = v if k == "act" else float(v)
            return out

        last = [state()["tick"]]
        steps = {"frame": {}, "key": {}}

        def count(kind):
            st = state()
            if int(st["tick"]) % 16 < 2:
                check_view(st)
            d = int(st["tick"] - last[0])
            steps[kind][d] = steps[kind].get(d, 0) + 1
            last[0] = st["tick"]
            return st

        def frames(n, until=None):
            """n frames, one at a time. Stops early when `until(state)`."""
            for _ in range(n):
                r.frame(1)
                st = count("frame")
                if until and until(st):
                    return st
            return state()

        def key(code, name, down):
            r.keyboard(code, name, down)
            count("key")

        def watch(n, until=None):
            """Like frames(), but returns every state seen."""
            seen = []
            for _ in range(n):
                r.frame(1)
                st = count("frame")
                seen.append(st)
                if until and until(st):
                    break
            return seen

        # --- S-1, and S-7's pictures: the stage builds a few a frame, then the dump lists them all.
        for _ in range(60):
            if find(r.dump_tree(item), "art:loaded")["text"].split("/")[0] == find(r.dump_tree(item), "art:loaded")["text"].split("/")[1]:
                break
            r.frame(1)
        last[0] = state()["tick"]  # the frames spent loading art are not a key's or a frame's ticks
        console = r.read_console(item)
        assert not console, console
        listed = pictures()
        assert set(listed) == ART and all(b > 0 for b in listed.values()), (sorted(set(listed) ^ ART), listed)
        print("stage pictures:", len(listed), "files,", sum(listed.values()), "bytes (level 0 as listed in the dump)")
        frames(4)
        st = state()
        assert st["y"] == 0 and st["vy"] == 0 and st["stocks"] == 3 and st["act"] == "idle", st
        r.save_screenshot(item, str(SHOTS / "step1.png"))
        r.save_screenshot(item, str(SHOTS / "step3_idle.png"))
        cam0 = readout("camera:view")
        assert cam0["zoom"] == cam0["zoom_max"], cam0 # alone, framed as close as the camera goes
        robot_check("idle", "step4_idle.png")
        facing_right = hover_row()

        # --- S-3, the full jump: Space held 40 frames.
        key("Space", " ", True)
        st = frames(20)
        r.save_screenshot(item, str(SHOTS / "step3_jump.png"))
        robot_check("air", "step4_air.png")
        st = frames(20)
        key("Space", " ", False)
        assert st["peak"] == 331.5, st
        # step 5: a landing with no key held is `land`, its boots on the floor
        st = frames(60, until=lambda st: st["act"] == "land" and st["act_t"] >= 2)
        robot_check("land", "step5_land.png")
        st = frames(60, until=lambda st: st["act"] == "idle")
        assert st["y"] == 0 and st["peak"] == 331.5, st

        # --- S-3, the short hop: a tap releases a few ticks after the press.
        key("Space", " ", True)
        key("Space", " ", False)
        st = frames(80, until=lambda st: st["act"] == "idle" and st["tick"] > 0)
        assert 107.25 <= st["peak"] < 331.5, st

        # --- S-2: run to speed, let go, run again and reverse (a skid first, then the facing flips).
        x0 = state()["x"]
        key("ArrowRight", "ArrowRight", True)
        st = frames(30)
        assert st["vx"] == RUN_SPEED and st["x"] > x0 and st["face"] == 1 and st["act"] == "run", st
        robot_check("run", "step4_run.png")
        key("ArrowRight", "ArrowRight", False)
        st = frames(10)
        assert st["vx"] == 0, st
        key("ArrowRight", "ArrowRight", True)
        frames(30)
        key("ArrowRight", "ArrowRight", False)
        key("ArrowLeft", "ArrowLeft", True)
        # the skid starts inside the key request (2-3 ticks of friction, then 4 or more is still a skid)
        seen = [state()] + watch(12)
        assert any(st["act"] == "turn" for st in seen), [(st["act"], st["vx"]) for st in seen]
        assert seen[-1]["face"] == -1 and seen[-1]["vx"] < 0 and seen[-1]["act"] == "run", seen[-1]
        frames(20)
        key("ArrowLeft", "ArrowLeft", False)
        st = frames(10)
        assert st["vx"] == 0 and st["x"] < 900 - 100, st
        st, _ = robot_check("mirrored", "step4_mirrored.png")
        assert st["face"] == -1 and st["act"] == "idle", st
        facing_left = hover_row()
        # S-8's mirror: facing left, the part under drawing point (u, v) is the one under (396 - u, v)
        # facing right. The idle clip breathes between the two scans, so most points, not all.
        both = [(u, v) for (u, v) in facing_right if facing_right[(u, v)] or facing_left[(ROBOT_SIZE[0] - u, v)]]
        same = [p for p in both if facing_right[p] == facing_left[(ROBOT_SIZE[0] - p[0], p[1])]]
        swapped = [p for p in facing_right if facing_right[p] != facing_left[p]]
        assert len(both) >= 20 and len(same) >= 0.85 * len(both), (len(same), len(both), facing_right, facing_left)
        assert len(swapped) >= 6, ("the parts did not swap sides", facing_right, facing_left)
        print(f"S-8 mirror: {len(same)}/{len(both)} points agree mirrored, {len(swapped)} swapped sides")

        # --- S-4: under the left platform, jump, stand on it; ArrowDown drops through.
        key("ArrowLeft", "ArrowLeft", True)
        st = frames(120, until=lambda st: st["x"] < -420)
        key("ArrowLeft", "ArrowLeft", False)
        st = frames(30, until=lambda st: st["vx"] == 0)
        assert LEFT_X[0] + 60 < st["x"] < LEFT_X[1] - 60 and st["y"] == 0, st
        key("Space", " ", True)
        frames(30) # held: a tap is a short hop now (step 2), which would not reach the platform
        key("Space", " ", False)
        st = frames(80, until=lambda st: st["act"] == "idle" and st["tick"] > 0)
        st = frames(10)
        assert st["y"] == LEFT_TOP and st["vy"] == 0 and st["act"] == "idle", st
        robot_check("platform", "step4_platform.png")
        key("ArrowDown", "ArrowDown", True)
        frames(1)
        key("ArrowDown", "ArrowDown", False)
        st = frames(80, until=lambda st: st["act"] == "idle")
        assert st["y"] == 0, st

        # --- S-6, at the far left of the ground, under no platform: the double jump, fast fall, dashes.
        key("ArrowLeft", "ArrowLeft", True)
        frames(200, until=lambda st: st["x"] < -760)
        key("ArrowLeft", "ArrowLeft", False)
        st = frames(30, until=lambda st: st["vx"] == 0)
        assert st["vx"] == 0 and st["y"] == 0 and st["x"] < -740, st
        # double jump: one per airtime, a third press does nothing, landing refreshes
        key("Space", " ", True)
        frames(10)
        key("Space", " ", False)
        key("Space", " ", True)
        seen = watch(2)
        assert seen[-1]["jumps2"] == 0 and any(st["act"] == "jump2" for st in seen), seen[-1]
        frames(6)
        assert state()["act"] == "jump2", state()
        robot_check("jump2", "step5_jump2.png")
        key("Space", " ", False)
        key("Space", " ", True)
        key("Space", " ", False)
        assert state()["jumps2"] == 0
        st = frames(200, until=lambda st: st["act"] == "idle")
        assert st["y"] == 0 and st["jumps2"] == 1, st
        # fast fall: from the apex of a jump, a fresh Down press goes to the cap and stays there
        key("Space", " ", True)
        frames(60, until=lambda st: st["vy"] > 0)
        frames(1)
        key("ArrowDown", "ArrowDown", True)
        seen = watch(60, until=lambda st: st["act"] == "idle")
        key("ArrowDown", "ArrowDown", False)
        key("Space", " ", False)
        assert seen[0]["vy"] == FAST_FALL and max(st["vy"] for st in seen) == FAST_FALL, [st["vy"] for st in seen]
        assert seen[-1]["y"] == 0, seen[-1]
        # dash on the ground: horizontal, 168 units at least, and free (the air dash is kept)
        key("ArrowRight", "ArrowRight", True)
        x0 = state()["x"]
        key("ShiftLeft", "Shift", True)
        key("ShiftLeft", "Shift", False)
        seen = watch(14)
        assert any(st["act"] == "dash" for st in seen) and seen[-1]["x"] - x0 >= 168, (x0, seen[-1])
        assert seen[-1]["dashes"] == 1 and seen[-1]["y"] == 0, seen[-1]
        key("ArrowRight", "ArrowRight", False)
        frames(10, until=lambda st: st["vx"] == 0)
        # dash in the air: spends the one dash, landing gives it back
        key("Space", " ", True)
        frames(6)
        key("ShiftLeft", "Shift", True)
        key("ShiftLeft", "Shift", False)
        seen = watch(4)
        assert any(st["act"] == "dash" for st in seen) and seen[-1]["dashes"] == 0, seen[-1]
        key("Space", " ", False)
        st = frames(200, until=lambda st: st["act"] == "idle")
        assert st["y"] == 0 and st["dashes"] == 1, st

        # --- S-5 and S-6's wall: off the right edge, back to the wall, down it, a wall jump; then out.
        key("ArrowRight", "ArrowRight", True)
        st = frames(300, until=lambda st: st["act"] == "fall")
        assert st["x"] > GROUND_EDGE, st
        r.save_screenshot(item, str(SHOTS / "step3_edge.png"))
        key("ArrowRight", "ArrowRight", False)
        key("ArrowLeft", "ArrowLeft", True)
        st = frames(200, until=lambda st: st["wall"] == -1)
        assert st["wall"] == -1 and st["dashes"] == 1, st
        seen = watch(6)
        assert all(st["act"] == "wall" and st["vy"] == WALL_SLIDE for st in seen[1:]), seen
        r.save_screenshot(item, str(SHOTS / "step2.png"))
        st, _ = robot_check("wall", "step5_wall.png")
        assert st["face"] == -1, ("a slide faces the wall", st)
        key("Space", " ", True)
        st = state()
        assert st["wall_jumps"] == 2 and st["jumps2"] == 1 and st["vx"] > 0 and st["vy"] < 0, st
        key("Space", " ", False)
        key("ArrowLeft", "ArrowLeft", False)
        st = frames(400, until=lambda st: st["act"] == "out")
        assert st["act"] == "out" and st["stocks"] == 2, st
        went_out = st["tick"] - st["act_t"]
        st = frames(120, until=lambda st: st["act"] != "out")
        back = st["tick"] - st["act_t"]
        assert back - went_out == RESPAWN_TICKS, (went_out, back, st)
        assert st["x"] == RESPAWN[0] and RESPAWN[1] <= st["y"] < RESPAWN[1] + 2 and st["stocks"] == 2, st
        assert st["wall_jumps"] == 3 and st["jumps2"] == 1 and st["dashes"] == 1, st
        # The respawn point is over the top platform: it lands there.
        st = frames(200, until=lambda st: st["act"] == "idle")
        assert st["y"] == TOP_TOP and st["x"] == 0, st

        # --- S-10: no shimmer after a stop. Run a few frames, stop on the platform, let the camera settle
        # (within 0.5 px it lands on its target): screenshots a few frames apart are the same pixels.
        key("ArrowRight", "ArrowRight", True)
        frames(8)
        key("ArrowRight", "ArrowRight", False)
        st = frames(30, until=lambda st: st["act"] == "idle")
        assert st["act"] == "idle" and st["y"] == TOP_TOP, st
        frames(90)  # past the camera's settle (about 1.4 s from the farthest) and the idle blend
        # The robot's own breathing animates (a different frame now and then), so the check is on
        # everything outside its rect and below the drifting clouds (rows < 160), and the rect must not move.
        shots, boxes = [], []
        for i in range(3):
            frames(3)
            path = SHOTS / f"s10_still{i}.png"
            r.save_screenshot(item, str(path))
            shots.append(Image(path.read_bytes()))
            rb = {x["id"]: x for x in r.rects()}["fighter:robot"]
            boxes.append((rb["x"], rb["y"], rb["w"], rb["h"]))
        assert boxes[0] == boxes[1] == boxes[2] and all(float(v).is_integer() for v in boxes[0][:2]), ("S-10", boxes)
        x0, y0, bw, bh = (int(v) for v in boxes[0])
        for img in shots[1:]:
            for y in range(160, img.height):
                ra, rb_ = shots[0].rows[y], img.rows[y]
                if y0 - 2 <= y <= y0 + bh + 2:
                    ra, rb_ = ra[: (x0 - 2) * 4] + ra[(x0 + bw + 2) * 4:], rb_[: (x0 - 2) * 4] + rb_[(x0 + bw + 2) * 4:]
                assert ra == rb_, f"S-10: row {y} of a still scene differs between frames"

        # --- S-9: the scripted timeline twice; the same checkpoint hashes and final state both times.
        def scripted_run():
            r.click(item, "script")
            r.frame(2)
            assert find(r.dump_tree(item), "hud:script"), "no scripted run"
            for _ in range(80):
                r.frame(10)
                if state()["tick"] >= SCRIPT_TICKS:
                    break
            r.frame(10)  # the hash of the last second needs a frame past it
            node = find(r.dump_tree(item), "fighter:state")
            final = node["text"]
            checks = {}
            for pair in find(r.dump_tree(item), "match:checks")["text"].split():
                frame, h = pair.split("=")
                if int(frame) <= SCRIPT_TICKS:
                    checks[int(frame)] = h
            return final, checks

        first, first_checks = scripted_run()
        second, second_checks = scripted_run()
        assert first == second, (first, second)
        assert first_checks == second_checks and len(first_checks) >= 9, (first_checks, second_checks)
        expect = lua_final()
        if expect:
            assert first == expect, (first, expect)
        else:
            print("SKIPPED the comparison with P-M12's final state: no lua on PATH")
        print("scripted run ok:", first)
        r.click(item, "play")
        r.frame(2)

        f, k = steps["frame"], steps["key"]
        assert set(f) <= {1, 2} and f.get(1, 0) * 20 < f[2], f"ticks per rpc.frame(1): {f}"
        assert set(k) <= {2, 3}, f"ticks per rpc.keyboard: {k}"
        console = r.read_console(item)
        assert not console, console
        assert view_checks[0] >= 20, f"only {view_checks[0]} frame-inside-playfield checks ran"
        print(f"S-7: {view_checks[0]} fighter-in-playfield checks")
        print(f"brawl smoke ok; ticks per frame(1) {f}, per keyboard {k}")


if __name__ == "__main__":
    main()
