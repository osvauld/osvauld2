"""Gravity lab on the real path: a real pointer drags the sling and lets go; the stone brings the
castle down, the dominoes run, the marble rolls into the bucket and the score says so. Then the
toys — wrecking ball, bomb, gravity, a hand throwing a crate — and the same run twice, to the
same pixel's worth of position. Read from the dump, not from pixels."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from osvauld.session import Session, shell_binary

ROOT = Path(__file__).resolve().parent.parent


def entities(tree):
    if tree.get("id") == "lab":
        return {e["id"]: e for e in tree["world"]["entities"]}
    for child in tree.get("children", []):
        found = entities(child)
        if found:
            return found
    return None


def texts(tree):
    found = [tree["text"]] if isinstance(tree.get("text"), str) else []
    for child in tree.get("children", []):
        found += texts(child)
    return found


def open_lab(s):
    rpc = s.rpc
    rpc.signup("lab", "lab passphrase")
    ws = rpc.create_workspace("gravity lab")
    item = rpc.create_item(ws["id"], "Siege", "app")["id"]
    rpc.upload_folder(item, ROOT / "demo_apps" / "gravity_lab")
    rpc.open_item(item)
    rpc.frame(3)
    assert not rpc.read_console(item), rpc.read_console(item)
    lab = next(r for r in rpc.rects() if r["id"] == "lab")
    return item, lab["x"], lab["y"]


def pos(world, id):
    return tuple(world[id]["pos"])


def slingshot(rpc, ox, oy, pull=(-62, 38), hold=20):
    """The real gesture: press the stone, pull it back by steps, hold a beat, let go."""
    ax, ay = ox + 96, oy + 392
    rpc.move_to(ax, ay)
    rpc.press()
    for i in range(1, 11):
        rpc.move_to(ax + pull[0] * i / 10, ay + pull[1] * i / 10)
    rpc.frame(hold)
    rpc.release()


def siege():
    with Session(shell_binary=shell_binary(), offscreen=(1000, 700)) as s:
        rpc = s.rpc
        item, ox, oy = open_lab(s)

        def look():
            tree = rpc.dump_tree(item)
            return entities(tree), texts(tree)

        # It stands: under gravity, the castle settles a pixel or two and holds.
        start, _ = look()
        rpc.frame(180)
        rested, said = look()
        for id in ("sl0", "sr0", "sl4", "beam1", "beam2", "pig_roof", "d0", "d6", "marble"):
            dx, dy = (a - b for a, b in zip(pos(rested, id), pos(start, id)))
            assert abs(dx) < 4 and abs(dy) < 4, (id, pos(start, id), pos(rested, id))
        assert "score 0" in said and any("Pull the stone back" in t for t in said), said

        # The aim: the stone follows the hand and a dotted arc shows the shot.
        ax, ay = ox + 96, oy + 392
        rpc.move_to(ax, ay)
        rpc.press()
        for i in range(1, 11):
            rpc.move_to(ax - 62 * i / 10, ay + 38 * i / 10)
        rpc.frame(2)
        aimed, _ = look()
        assert aimed["ammo"]["pos"][0] < 60 and aimed["ammo"]["pos"][1] > 410, aimed["ammo"]
        arc = [aimed[f"arc{i}"]["pos"] for i in range(1, 17)]
        assert all(p[0] > 0 for p in arc) and arc[-1][0] > arc[0][0] + 300, arc
        rpc.release()
        rpc.frame(2)
        gone, _ = look()
        assert all(gone[f"arc{i}"]["pos"][0] < 0 for i in range(1, 17)), "the arc goes when the stone does"

        # The chain of causes, on the clock: castle, dominoes, marble, bucket.
        rpc.frame(60 * 8)
        end, said = look()
        assert abs(end["beam1"]["pos"][1] - start["beam1"]["pos"][1]) > 20, ("the castle fell", end["beam1"])
        assert end["d0"]["rot"] > 20 and end["d6"]["rot"] > 10, ("the dominoes ran", end["d0"], end["d6"])
        assert end["marble"]["pos"][0] > 860, ("the marble rolled down the ramp", end["marble"])
        assert any("GOAL" in t for t in said) and "score 600" in said, said
        assert "bucket" in end and "glow" in end, "the bucket lit up"
        assert not rpc.read_console(item), rpc.read_console(item)
        return {k: (round(end[k]["pos"][0], 1), round(end[k]["pos"][1], 1)) for k in ("beam1", "d0", "d6", "marble", "shot1")}


def toys():
    with Session(shell_binary=shell_binary(), offscreen=(1000, 700)) as s:
        rpc = s.rpc
        item, ox, oy = open_lab(s)

        def look():
            tree = rpc.dump_tree(item)
            return entities(tree), texts(tree)

        def say(*needles):
            said = " ".join(look()[1])
            for n in needles:
                assert n in said, (n, said)

        def reset(gen):
            rpc.click(item, "reset")
            rpc.frame(150)
            fresh, said = look()
            assert "score 0" in said and f"pig_roof~{gen}" in fresh, said
            assert abs(fresh[f"pig_roof~{gen}"]["pos"][0] - 478) < 6, "a fresh castle stands"
            return fresh

        rpc.frame(120)
        before, _ = look()
        # The wrecking ball: a kick swings the chain, and the links follow the ball.
        rpc.click(item, "swing")
        rpc.frame(60)
        swung, _ = look()
        assert swung["bob"]["pos"][0] > before["bob"]["pos"][0] + 40, ("the ball swung", swung["bob"])
        assert swung["link4"]["pos"][0] > before["link4"]["pos"][0] + 5, "the chain follows"
        say("Wrecking ball")
        # …but never past its rope: the ball stays within the chain's reach of the anchor.
        far = lambda e: ((e["pos"][0] + 22 - 390) ** 2 + (e["pos"][1] + 22 - 24) ** 2) ** 0.5
        for _ in range(8):
            rpc.frame(20)
            assert far(look()[0]["bob"]) < 26 * 10 + 6, far(look()[0]["bob"])

        # The bomb, on a fresh castle: everything near is thrown away from the blast.
        fresh = reset(1)
        rpc.click(item, "bomb")
        rpc.frame(6)
        boom, _ = look()
        say("BOOM")
        speed = lambda e: (e["velocity"][0] ** 2 + e["velocity"][1] ** 2) ** 0.5
        assert speed(boom["wl~1"]) > 80 and speed(boom["sr2~1"]) > 80, (boom["wl~1"]["velocity"], boom["sr2~1"]["velocity"])
        assert speed(boom["d6~1"]) < 1, "the dominoes, out of reach, did not move"
        assert boom["wl~1"]["pos"][1] < fresh["wl~1"]["pos"][1] - 5, "thrown up and away"

        # The well pulls on a fresh castle; closed, it stops.
        fresh = reset(2)
        rpc.click(item, "well")
        rpc.frame(60)
        welled, _ = look()
        say("Well: on")
        assert welled["wl~2"]["pos"] != fresh["wl~2"]["pos"] or speed(welled["wl~2"]) > 5, "the well drew on it"
        rpc.click(item, "well")
        say("Well: off")

        # Gravity cycles normal -> moon -> flip; flipped, things fall up and pile on the ceiling.
        fresh = reset(3)
        rpc.click(item, "gravity")
        say("Gravity: moon")
        rpc.click(item, "gravity")
        say("Gravity: flip")
        rpc.frame(60)
        up, _ = look()
        assert up["pig_roof~3"]["pos"][1] < fresh["pig_roof~3"]["pos"][1] - 30, ("it rose", fresh["pig_roof~3"], up["pig_roof~3"])
        rpc.frame(180)
        assert look()[0]["pig_roof~3"]["pos"][1] > -40, "and the ceiling held it"
        rpc.click(item, "gravity")
        say("Gravity: normal")

        # A hand: take the crate on the seesaw and throw it to the right.
        fresh = reset(4)
        crate = "seesaw_c1~4"
        e = fresh[crate]
        cx, cy = ox + e["pos"][0] + 11, oy + e["pos"][1] + 12
        rpc.move_to(cx, cy)
        rpc.press()
        for i in range(1, 9):
            rpc.move_to(cx + 25 * i, cy - 12 * i)
            rpc.frame(1)
        rpc.release()
        thrown = look()[0][crate]
        assert thrown["velocity"][0] > 150, ("let go, it keeps the hand's speed", thrown["velocity"])
        rpc.frame(30)
        assert look()[0][crate]["pos"][0] > e["pos"][0] + 100, "and it flew on"
        assert not rpc.read_console(item), rpc.read_console(item)


if __name__ == "__main__":
    first = siege()
    print("siege ok:", first)
    second = siege()
    assert first == second, ("the same run is the same run", first, second)
    print("deterministic: the second run matched")
    toys()
    print("toys ok")
