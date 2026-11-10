"""Drive the rover on an imported moon surface (blender-import.md E3/E4): a ~200k-vertex GLB
terrain and boulders become fixed mesh bodies by a Lua rule; real keys drive the rover over it."""
from pathlib import Path
import math
import subprocess
import sys

from osvauld.scene3d import assert_3d_pixels
from osvauld.session import Session, shell_binary

ROOT = Path(__file__).resolve().parent.parent
GLB = ROOT / "demo_apps/moon_rover/models/moon_surface.glb"
if not GLB.exists():
    subprocess.run([sys.executable, "-I", str(ROOT / "scripts/fixtures/make_moon.py")], check=True)

with Session(shell_binary=shell_binary(), offscreen=(1000, 760)) as s:
    r = s.rpc
    r.signup("moon", "moon surface smoke passphrase")
    ws = r.create_workspace("Moon")
    item = r.create_item(ws["id"], "Moon rover", "app")["id"]
    r.upload_folder(item, ROOT / "demo_apps/moon_rover")
    r.open_item(item)
    r.click_at(*r.centre_of("surface-enter"))
    r.frame(2)

    def read():
        tree = r.dump_tree(item)
        assert "worlds3d" in tree and "moon-surface" in tree["worlds3d"], (tree.get("worlds3d", {}).keys(), r.read_console(item))
        return tree["worlds3d"]["moon-surface"]

    def rover(state):
        return next(e["resolved"] for e in state["entities"] if e["authored"]["id"] == "rover")

    state = read()
    ids = {e["authored"]["id"] for e in state["entities"]}
    assert "ground" in ids and sum(i.startswith("boulder-") for i in ids) == 40, sorted(ids)
    r.click_at(*r.centre_of("surface-drive"))
    r.frame(180)
    settled = read()
    assert all(w["contact"] for w in settled["wheels"]), settled["wheels"]
    start = rover(settled)["position"]
    assert abs(start[1]) < 1.5, start
    def view_objects():
        tree = r.dump_tree(item)
        stack = [tree]
        while stack:
            n = stack.pop()
            if n.get("id") == "surface-view":
                return {o["id"]: o for o in n["scene3d"]["objects"]}
            stack += n.get("children", [])
        raise AssertionError("no surface-view")

    # Whichever rover model is present: E-2000 parts ride the chassis and wheels via `attach`,
    # Curiosity's ride the chassis, else the boxy rover's id-bound visual.
    present = view_objects()
    visual = next(i for i in ("COCKPIT", "curiosity:MSL", "rover") if i in present)
    before = view_objects()[visual]["position"]
    assert_3d_pixels(r, item, "surface-view", visual, logical_width=1000)

    def arm_angle():  # degrees between an aimed arm and the chassis-rigid cockpit
        objs = view_objects()
        if "FRONT_ARM" not in objs:
            return None
        a, b = objs["FRONT_ARM"]["rotation"], objs["COCKPIT"]["rotation"]
        return math.degrees(2 * math.acos(min(1.0, abs(sum(x * y for x, y in zip(a, b))))))

    angles = [arm_angle()]
    r.keyboard("KeyW", "w", True)
    for _ in range(6):
        r.frame(50)
        angles.append(arm_angle())
    r.keyboard("KeyD", "d", True)
    r.frame(120)
    angles.append(arm_angle())
    r.keyboard("KeyD", "d", False)
    r.frame(120)
    angles.append(arm_angle())
    r.keyboard("KeyW", "w", False)
    driven = read()
    end = rover(driven)["position"]
    travelled = math.hypot(end[0] - start[0], end[2] - start[2])
    assert travelled > 8, (start, end)
    after = view_objects()[visual]["position"]
    moved = math.hypot(after[0] - before[0], after[2] - before[2])
    assert abs(moved - travelled) < 1.5, (visual, moved, travelled)
    if "WHEEL.001" in present:  # E-2000: its wheel parts take the native wheel poses
        objs = view_objects()
        poses = [w["position"] for w in driven["wheels"]]
        for name in ("WHEEL.001", "WHEEL.002", "WHEEL.003", "WHEEL.004"):
            at = objs[name]["position"]
            assert min(math.dist(at, p) for p in poses) < 1e-3, (name, at, poses)
    if angles[0] is not None:  # E-2000: arms follow the suspension instead of riding rigidly
        assert max(angles) - min(angles) > 0.3, ("arms did not articulate", angles)
        print("arm swing (deg):", " ".join(f"{a:.2f}" for a in angles))
    q = rover(driven)["rotation"]
    assert 1 - 2 * (q[0] ** 2 + q[2] ** 2) > 0.5, ("rover flipped", q)
    r.save_screenshot(item, ROOT / "shots/moon-surface.png")
    print(f"moon surface ok: drove {travelled:.1f} m to {end}; {visual} followed")
