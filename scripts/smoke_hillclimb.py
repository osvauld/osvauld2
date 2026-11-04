"""Hillclimb on the real path (plan: docs/design/hillclimb.md §0, S-1..S-6): upload the app, let
the buggy settle on its springs, hold the on-screen gas pedal with a real pointer and drive —
fuel cans and coins taken, the camera following — until the run ends, then Retry. The same drive
twice ends in the same place. Read from the dump and the HUD labels, not from pixels."""

import os
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from osvauld.session import Session, shell_binary

ROOT = Path(__file__).resolve().parent.parent
SHOTS = Path(os.environ.get("HILLCLIMB_SHOTS", "/tmp"))


def find(tree, pred):
    if pred(tree):
        return tree
    for child in tree.get("children", []):
        found = find(child, pred)
        if found:
            return found
    return None


def texts(tree):
    found = [tree["text"]] if isinstance(tree.get("text"), str) else []
    for child in tree.get("children", []):
        found += texts(child)
    return found


def by_id(tree, id):
    node = find(tree, lambda n: n.get("id") == id)
    return node.get("text") if node else None


class Game:
    def __init__(self, s):
        self.rpc = rpc = s.rpc
        rpc.signup("driver", "driver passphrase")
        ws = rpc.create_workspace("hills")
        self.item = rpc.create_item(ws["id"], "Hillclimb", "app")["id"]
        rpc.upload_folder(self.item, ROOT / "demo_apps" / "hillclimb")
        rpc.open_item(self.item)
        rpc.frame(3)
        self.quiet()

    def quiet(self):
        console = self.rpc.read_console(self.item)
        assert not console, console

    def look(self):
        tree = self.rpc.dump_tree(self.item)
        world = find(tree, lambda n: str(n.get("id", "")).startswith("stage") and "world" in n)
        assert world, "the stage world is in the dump"
        return {e["id"]: e for e in world["world"]["entities"]}, world["world"], tree

    def number(self, tree, id):
        text = by_id(tree, id)
        assert text, (id, texts(tree))
        return float(re.search(r"-?[\d.]+", text).group())

    def hold(self, pedal, frames, every=30, watch=None):
        """A real press on the pedal, held for `frames`, then let go; `watch` sees each sample."""
        r = next(r for r in self.rpc.rects() if r["id"] == "pedal_" + pedal)
        self.rpc.move_to(r["x"] + r["w"] / 2, r["y"] + r["h"] / 2)
        self.rpc.press()
        done = 0
        while done < frames:
            self.rpc.frame(min(every, frames - done))
            done += every
            if watch and watch(*self.look()):
                break
        self.rpc.release()
        self.rpc.frame(1)


def drive(shots=False):
    with Session(shell_binary=shell_binary(), offscreen=(900, 700)) as s:
        g = Game(s)
        rpc = g.rpc

        # S-1: it settles on its springs, level, at the start.
        rpc.frame(240)
        ents, world, tree = g.look()
        c, rear = ents["chassis"], ents["wheel1"]
        assert abs(c["rot"]) < 1.5 and abs(c["velocity"][0]) < 3, c
        # Unloaded the wheel's centre is 62 below the chassis top; loaded, 290 kg / 2 at 18000 N/m.
        compression = 62 - (rear["pos"][1] + 32 - c["pos"][1])
        assert abs(compression - 7.9) < 2.5, ("sag on the springs", compression)
        assert g.number(tree, "distance") == 0 and g.number(tree, "coins") == 0
        start_x = c["pos"][0]
        if shots:
            rpc.save_screenshot(g.item, SHOTS / "hillclimb_start.png")

        # S-2: the gas pedal, held by a real pointer, drives it off; the HUD and camera keep up.
        g.hold("gas", 300)
        ents, world, tree = g.look()
        c = ents["chassis"]
        assert c["pos"][0] > start_x + 1500, ("the gas drove it on", start_x, c)
        cam = world["camera"]
        assert cam["follow"] == "chassis" and not cam["lost"], cam
        assert abs(c["pos"][0] + 110 - cam["at"][0]) < 450, ("the camera keeps the car in view", cam["at"], c["pos"])
        metres = g.number(tree, "distance")
        assert abs(metres - (c["pos"][0] - start_x) / 100) < 3, (metres, c["pos"])
        g.quiet()

        # S-3/S-4: keep the gas down over the hills; fuel is burnt and refilled, coins counted.
        fuels, ended = [], {}

        def watch(ents, world, tree):
            fuels.append(g.number(tree, "fuel"))
            over = by_id(tree, "over")
            if over:
                ended["why"] = over
            return bool(over)

        g.hold("gas", 60 * 90, watch=watch)
        ents, world, tree = g.look()
        coins, metres = g.number(tree, "coins"), g.number(tree, "distance")
        print(f"  drove {metres:.0f} m, {coins:.0f} coins, ended: {ended.get('why')}, fuel low {min(fuels):.1f}")
        assert metres > 150, ("over the first hills", metres)
        assert coins >= 5, ("coins taken on the way", coins)
        lows = [i for i in range(1, len(fuels)) if fuels[i] > fuels[i - 1] + 1]
        assert lows, ("a fuel can filled the tank", fuels)
        if shots:
            rpc.save_screenshot(g.item, SHOTS / "hillclimb_drive.png")

        # S-5: the run ends (head, roof, empty tank or the finish); the panel says how far, Retry
        # starts a fresh car at the start.
        if not ended:
            g.hold("brake", 60 * 30, watch=watch)
        assert ended, "the run ended"
        ents, world, tree = g.look()
        assert by_id(tree, "result").startswith(f"{int(g.number(tree, 'distance'))} m"), texts(tree)
        final = {k: (round(e["pos"][0], 1), round(e["pos"][1], 1), round(e["rot"], 1)) for k, e in ents.items()
                 if k in ("chassis", "wheel1", "wheel2", "head")}
        if shots:
            rpc.save_screenshot(g.item, SHOTS / "hillclimb_over.png")
        rpc.click(g.item, "retry")
        rpc.frame(60)
        ents, world, tree = g.look()
        assert world is not None and abs(ents["chassis"]["pos"][0] - start_x) < 5, ents["chassis"]
        assert g.number(tree, "distance") == 0 and by_id(tree, "over") is None
        g.quiet()

        # The road's left end is a wall: reversing from the start stops there, it doesn't fall off.
        start_y = ents["chassis"]["pos"][1]
        g.hold("brake", 600)
        ents, world, tree = g.look()
        c = ents["chassis"]
        assert c["pos"][0] > -150 and abs(c["pos"][1] - start_y) < 300, ("stopped by the wall", c)
        g.quiet()
        return ended["why"], final


def pure_tests():
    """The vehicle setup and run rules in plain Lua: no shell, but run here so they are run."""
    import subprocess
    for test in ("test_hillclimb_vehicle.lua", "test_hillclimb_run.lua"):
        out = subprocess.run(["lua", f"scripts/{test}"], cwd=ROOT, capture_output=True, text=True)
        assert out.returncode == 0, (test, out.stdout, out.stderr)
        print(" ", out.stdout.strip())


if __name__ == "__main__":
    pure_tests()
    why, first = drive(shots=bool(os.environ.get("HILLCLIMB_SHOTS")))
    print("drive ok:", why, first)
    # S-6: the same pointer, the same frames: the same place.
    _, second = drive()
    assert first == second, ("deterministic", first, second)
    print("deterministic: the second drive matched")
