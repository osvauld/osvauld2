"""One mesh drives visible lunar terrain and collisions, through the real Lua/bridge path."""
from pathlib import Path
import tempfile

from osvauld.scene3d import assert_3d_pixels, project
from osvauld.session import Session, shell_binary

ROOT = Path(__file__).resolve().parent.parent


def nodes(tree):
    yield tree
    for child in tree.get("children", []):
        yield from nodes(child)


def read(rpc, item):
    tree = rpc.dump_tree(item)
    assert "worlds3d" in tree, (tree, rpc.read_console(item))
    return tree["worlds3d"]["lunar-course"]


def body(state, name):
    return next(e for e in state["entities"] if e["authored"]["id"] == name)


def edit(rpc, item, path, old, new):
    file = rpc.read_file_versioned(item, path)
    return rpc.edit_file(item, path, file["revision"], [{"old_text": old, "new_text": new}])


with tempfile.TemporaryDirectory(prefix="lunar-terrain-") as tmp:
    data = str(Path(tmp) / "store")
    with Session(shell_binary=shell_binary(), data_dir=data, offscreen=(900, 700)) as s:
        rpc = s.rpc
        rpc.signup("terrain", "terrain smoke passphrase")
        ws = rpc.create_workspace("Moon course")
        item = rpc.create_item(ws["id"], "Lunar terrain", "app")["id"]
        rpc.upload_folder(item, ROOT / "demo_apps/moon_rover")
        rpc.open_item(item)
        ready = read(rpc, item)
        scene = next(n["scene3d"] for n in nodes(rpc.dump_tree(item)) if n.get("id") == "course-view")
        visual = next(o for o in scene["objects"] if o["id"] == "course")
        collision = body(ready, "course")["authored"]["shape"]["mesh"]
        assert collision["resource"] == visual["mesh_resource"], (collision, visual)
        assert collision["triangles"] == visual["triangles"] == 6144, (collision, visual)
        assert_3d_pixels(rpc, item, "course-view", "course")
        rpc.frame(30)
        assert read(rpc, item) == ready, "Ready must remain paused"
        rpc.save_screenshot(item, ROOT / "shots/moon-course-ready.png")
        rpc.click_at(*rpc.centre_of("course-release"))
        for _ in range(48):
            rpc.frame(10)
            current = read(rpc, item)
            x, y, z = body(current, "ball")["resolved"]["position"]
            assert -6 < x < 2 and abs(z) < 0.05, current
            surface = -0.2 * min(4, max(0, x + 6))
            assert y >= surface + 0.30, f"ball sank through terrain: {current}"
        assert x > -1.8, f"ball did not cross the slope/flat join: {current}"
        box = body(current, "box")["resolved"]
        assert box["sleeping"] and abs(box["position"][1] - 0.2) < 0.03, box
        rpc.click_at(*rpc.centre_of("course-pause"))
        paused = read(rpc, item)
        rpc.frame(60)
        rpc.save_screenshot(item, ROOT / "shots/moon-course-settled.png")
        rpc.screenshot(item, width=1100, height=800, scale=1)
        assert read(rpc, item) == paused, "pause/capture cannot advance physics"
        for old, new in [
            ("local height_scale = 1", "local height_scale = 1.5"),
            ("indices[#indices + 1] = index", "indices[#indices + 1] = 0"),
        ]:
            failed = edit(rpc, item, "course.lua", old, new)
            assert failed["persisted"] and "failed" in failed["activation"], failed
            assert read(rpc, item) == paused, "failed terrain edit changed live physics"
            fixed = edit(rpc, item, "course.lua", new, old)
            assert fixed["activation"] == "activated", fixed
            after = read(rpc, item)
            assert after["tick"] == paused["tick"]
            for identity in ("ball", "box", "course"):
                assert body(after, identity)["resolved"] == body(paused, identity)["resolved"]
            paused = after
        rpc.click_at(*rpc.centre_of("course-reset"))
        reset = read(rpc, item)
        assert body(reset, "ball")["resolved"]["position"] == [-4, 2.4000000953674316, 0]
        assert body(reset, "ball")["resolved"]["velocity"] == [0, 0, 0]
        rpc.click_at(*rpc.centre_of("course-overview"))
        view = next(n["scene3d"] for n in nodes(rpc.dump_tree(item)) if n.get("id") == "course-view")
        rect = next(r for r in rpc.rects() if r["id"] == "course-view")
        ball = body(reset, "ball")["resolved"]["position"]
        x, y = project(view["camera"], ball, rect)
        rpc.click_at(x, y)
        focused = next(n["scene3d"] for n in nodes(rpc.dump_tree(item)) if n.get("id") == "course-view")
        assert sum((a-b)**2 for a, b in zip(focused["camera"]["target"], ball)) < 0.5, focused
        cx, cy = rpc.centre_of("course-view")
        rpc.move_to(cx, cy)
        rpc.wheel(cx, cy, 0, -1200)
        close = next(n["scene3d"] for n in nodes(rpc.dump_tree(item)) if n.get("id") == "course-view")
        assert sum((a-b)**2 for a, b in zip(close["camera"]["eye"], close["camera"]["target"])) < 4, close
        rpc.save_screenshot(item, ROOT / "shots/moon-course-close.png")
        rpc.click_at(*rpc.centre_of("course-overview"))
        assert read(rpc, item) == reset, "camera operations cannot move paused physics"
        def camera():
            return next(n["scene3d"]["camera"] for n in nodes(rpc.dump_tree(item)) if n.get("id") == "course-view")
        before = camera()
        rpc.keyboard("KeyW", "w", True)
        rpc.frame(30)
        rpc.keyboard("KeyW", "w", False)
        rpc.frame(1)
        moved = camera()
        assert sum((a-b)**2 for a, b in zip(moved["target"], before["target"])) > 1, (before, moved)
        rpc.keyboard("ArrowRight", "ArrowRight", True)
        rpc.frame(20)
        rpc.keyboard("ArrowRight", "ArrowRight", False)
        rpc.frame(1)
        turned = camera()
        assert turned["target"] == moved["target"]
        assert sum((a-b)**2 for a, b in zip(turned["eye"], moved["eye"])) > 1, (moved, turned)
        rpc.keyboard("KeyE", "e", True)
        rpc.frame(30)
        rpc.keyboard("KeyE", "e", False)
        rpc.frame(1)
        zoomed = camera()
        assert sum((a-b)**2 for a, b in zip(zoomed["eye"], zoomed["target"])) < 500, zoomed
        assert read(rpc, item) == reset, "keyboard camera navigation cannot move paused physics"
        for preset in ("flat", "ramp", "bumps", "crater"):
            rpc.click_at(*rpc.centre_of("course-view-" + preset))
        rpc.save_screenshot(item, ROOT / "shots/moon-course-crater.png")
        rpc.keyboard("KeyF", "f", True)
        rpc.keyboard("KeyF", "f", False)
        assert camera()["target"] == [0, -0.5, 0]
        rpc.click_at(*rpc.centre_of("course-view-ground"))
        grounded = camera()
        rpc.keyboard("KeyW", "w", True)
        rpc.frame(90)
        rpc.keyboard("KeyW", "w", False)
        rpc.frame(1)
        walked = camera()
        x, y, z = walked["eye"]
        ground_height = -0.2 * min(4, max(0, x + 6))
        assert abs(y - ground_height - 1.6) < 0.01, walked
        assert x > grounded["eye"][0] + 1, (grounded, walked)
        assert y < grounded["eye"][1] - 0.1, (grounded, walked)
        assert read(rpc, item) == reset, "ground camera movement cannot move physical probes"
        rpc.save_screenshot(item, ROOT / "shots/moon-course-ground.png")
        rpc.keyboard("Space", " ", True)
        rpc.keyboard("Space", " ", False)
        rpc.frame(10)
        assert read(rpc, item)["tick"] > reset["tick"], "Space did not release the probes"
        rpc.keyboard("KeyR", "r", True)
        rpc.keyboard("KeyR", "r", False)
        key_reset = read(rpc, item)
        reset_ball = body(key_reset, "ball")["resolved"]
        assert abs(reset_ball["position"][1] - 2.4) < 0.01, reset_ball
        assert max(abs(v) for v in reset_ball["velocity"]) < 0.05, reset_ball
        rpc.frame(20)
        assert read(rpc, item) == key_reset, "keyboard reset must leave the world paused"
        rpc.lock()
    with Session(shell_binary=shell_binary(), data_dir=data, offscreen=(900, 700)) as s:
        rpc = s.rpc
        rpc.unlock("terrain", "terrain smoke passphrase")
        rpc.open_item(item)
        assert body(read(rpc, item), "course")["authored"]["shape"]["mesh"]["triangles"] == 6144
        assert not rpc.read_console(item), rpc.read_console(item)
    print("lunar terrain ok: shared mesh, drop, slope/join, pause, captures, failed edits, reset and restart")
