"""Brawl fighting, step 1, through the shell (plan: docs/design/brawl-fighting.md §0).

S-F1: the app opens on the training dummy: the mode button reads `Dummy`, two fighters, `hud:dmg:1`
and `hud:dmg:2` read 0%, the dummy stands at spawn 2. Screenshot.
S-F2's numbers: by keys, walk up to the dummy and press J: `fight:state:2` gives dmg=8 and P-F5's
last_kb (5.5); a side K raises it more. With F2 on, `hit:1` is in rects() only on a move's active
ticks, and `hurt:<i>` covers each fighter's box. Screenshots of an active frame.
S-F6: the dummy's percent set to 150 (hidden `dummy:pct`), a side K near the edge: its outs go up by
one and it comes back invulnerable with dmg=0.
S-F9: dodges by keys: L is a spot dodge (`dodge_spot`, `inv` > 0 on some frames, back to idle, x unchanged,
a shot while invulnerable); L again inside the 48-tick cooldown does nothing, after it dodges again;
L with a side held moves 131 (128 + the tail; the run before the press, if any, adds a few) and keeps the facing.
S-F2 (step 3): every move by keys plays its own clip (`look:state`), and its first active frame is shot
with the circles drawn (`shots/brawl/fight/s_f2_<move>_active.png`).
S-F4 (step 4): the jet by keys: in the air K held is `jet`, then `jet_hover` with the fuel falling and
`look:state` flame=1; Up gives flame=2, a side `jet_thrust`; letting go is `jet_cut`; landing refills
to 120. Shots jet_ignite/hover/thrust/cut.png.
S-F5 (step 5): the mode button goes Dummy -> CPU; shots of a CPU match (both robots, the CPU attacking)
and of the hidden `vs:bots` (CPU vs CPU) once one is knocked off and jets back; the hidden `script:cpu` (keys idle, 3 600
ticks) ends on the final `fight:state` lines scripts/test_brawl_bot.lua prints (P-CPU5) with the human a
stock down; `vs:bot` played to the end reads `CPU wins — press Enter`, and Enter restarts.
S-F3: the hidden `script:fight` button runs script_fight.lua twice: the same `match:checks` both
times, and the final `fight:state` lines equal those scripts/test_brawl_fight.lua prints (P-F13).
Facts here don't depend on which tick a key lands on; per-tick facts are the Lua tests'."""

import shutil
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from osvauld.session import Session, shell_binary

ROOT = Path(__file__).resolve().parent.parent
SHOTS = ROOT / "shots" / "brawl" / "fight"
SPAWN2 = (450.0, 0.0)
FIGHT_TICKS = 1800
# moves.lua: the move ticks each move's windows cover (1-based; act_t + 1 is the move tick)
WINDOWS = {"nlight": [(11, 16)], "slight": [(13, 18)], "dlight": [(11, 16)], "nheavy": [(23, 30)], "sheavy": [(25, 32)],
           "dheavy": [(21, 30)], "nair": [(9, 12), (17, 20)], "sair": [(11, 16)], "dair": [(11, 18)], "shair": [(15, 22)],
           "dhair": [(15, 74)]}


def find(tree, identity):
    if isinstance(tree, dict):
        if tree.get("id") == identity:
            return tree
        tree = tree.get("children", [])
    for child in tree:
        found = find(child, identity)
        if found:
            return found


def lua_finals(test="scripts/test_brawl_fight.lua", prefix="P-F13 final "):
    """The final fight:state lines a Lua test's straight run prints, or None without a Lua."""
    lua = shutil.which("lua5.4") or shutil.which("lua")
    if not lua:
        return None
    out = subprocess.run([lua, test], cwd=ROOT, capture_output=True, text=True, check=True).stdout
    finals = {}
    for line in out.splitlines():
        if line.startswith(prefix):
            i, rest = line[len(prefix):].split(": ", 1)
            finals[int(i)] = rest
    assert len(finals) == 2, out
    return finals


def main():
    SHOTS.mkdir(parents=True, exist_ok=True)
    with Session(shell_binary=shell_binary(), offscreen=(900, 700)) as s:
        r = s.rpc
        r.signup("brawler", "brawl fight smoke passphrase")
        ws = r.create_workspace("Brawl fight smoke")
        item = r.create_item(ws["id"], "Brawl", "app")["id"]
        r.upload_folder(item, ROOT / "demo_apps/brawl")
        r.open_item(item)
        r.frame(2)

        def text(name, tree=None):
            node = find(tree or r.dump_tree(item), name)
            assert node, (name, r.read_console(item))
            return node["text"]

        def parse(t):
            return {k: (v if k == "act" else float(v)) for k, v in (p.split("=") for p in t.split())}

        def fs(i, tree=None):
            return parse(text(f"fight:state:{i}", tree))

        def frames(n, until=None):
            for _ in range(n):
                r.frame(1)
                if until:
                    tree = r.dump_tree(item)
                    if until(fs(1, tree), fs(2, tree)):
                        break
            return fs(1), fs(2)

        def walk_to(x, code="ArrowRight"):
            r.keyboard(code, code, True)
            frames(300, lambda a, d: a["x"] >= x if code == "ArrowRight" else a["x"] <= x)
            r.keyboard(code, code, False)
            return frames(30, lambda a, d: parse(text("fighter:state"))["vx"] == 0)

        def tap(code, key, held=None):
            if held:
                r.keyboard(held, held, True)
            r.keyboard(code, key, True)
            r.frame(1)
            r.keyboard(code, key, False)
            if held:
                r.keyboard(held, held, False)

        # The art loads a few pictures a frame.
        for _ in range(80):
            n, total = text("art:loaded").split("/")
            if n == total:
                break
            r.frame(1)
        frames(6)

        # --- S-F1
        console = r.read_console(item)
        assert not console, console
        tree = r.dump_tree(item)
        assert text("mode:label", tree) == "Dummy", text("mode:label", tree)
        assert find(tree, "fight:state:2") and not find(tree, "fight:state:3"), "two fighters"
        assert text("hud:dmg:1", tree) == "0%" and text("hud:dmg:2", tree) == "0%", (text("hud:dmg:1", tree), text("hud:dmg:2", tree))
        d = fs(2, tree)
        assert (d["x"], d["y"]) == SPAWN2 and d["stocks"] == -1 and d["act"] == "idle", d
        ids = {x["id"] for x in r.rects()}
        assert {"fighter", "fighter:robot", "fighter:2", "fighter:2:robot", "mode"} <= ids, sorted(ids)
        r.save_screenshot(item, str(SHOTS / "s_f1_open.png"))
        print("S-F1: mode Dummy, two fighters at 0%, the dummy at spawn 2", d["x"], d["y"])

        # --- S-F2: walk up, J; F2 on, the overlay while it swings.
        r.keyboard("F2", "F2", True)
        r.keyboard("F2", "F2", False)
        assert text("overlay:state") == "on"
        a, d = walk_to(SPAWN2[0] - 95)
        assert d["x"] - a["x"] < 102, ("in nlight's reach", a, d)
        seen_hit, active_shot = 0, False
        r.keyboard("KeyJ", "j", True)
        r.keyboard("KeyJ", "j", False)
        for _ in range(40):
            tree = r.dump_tree(item)
            rects = {x["id"]: x for x in r.rects()}
            a, d = fs(1, tree), fs(2, tree)
            for i, box in ((1, "fighter"), (2, "fighter:2")):
                if box in rects:
                    h, b = rects[f"hurt:{i}"], rects[box]
                    assert all(abs(h[k] - b[k]) <= 1.0 for k in ("x", "y", "w", "h")), (i, h, b)
            k = a["act_t"] + 1
            live = a["act"] in WINDOWS and any(lo <= k <= hi for lo, hi in WINDOWS[a["act"]])
            assert ("hit:1" in rects) == live, (a, "hit:1" in rects)
            if live:
                seen_hit += 1
                if not active_shot:
                    r.save_screenshot(item, str(SHOTS / "s_f2_nlight_active.png"))
                    active_shot = True
            if a["act"] == "idle" and d["dmg"] > 0:
                break
            r.frame(1)
        assert seen_hit >= 1, "hit:1 never appeared"
        a, d = frames(60, lambda a, d: d["act"] == "idle" and d["dmg"] > 0)
        assert d["dmg"] == 8 and d["last_kb"] == 5.5 and d["last_angle"] == 30 and d["last_stun"] == 11, d
        assert text("hud:dmg:2") == "8%", text("hud:dmg:2")
        print(f"S-F2: nlight: dmg {d['dmg']:g}, last_kb {d['last_kb']:g}, angle {d['last_angle']:g}, stun {d['last_stun']:g}; hit:1 on {seen_hit} sampled frames")
        # side K: sheavy, with the lunge
        walk_to(d["x"] - 170)
        before = fs(2)
        tap("KeyK", "k", held="ArrowRight")
        shot = False
        for _ in range(60):
            r.frame(1)
            a = fs(1)
            if a["act"] == "sheavy" and 25 <= a["act_t"] + 1 <= 32 and not shot:
                r.save_screenshot(item, str(SHOTS / "s_f2_sheavy_active.png"))
                shot = True
        a, d = frames(200, lambda a, d: d["act"] == "idle" and a["act"] == "idle")
        # sheavy: kb = 7 + 11/64 * dmg. At 28 % it carries ~430 units, so a dummy near the ledge flies off
        # it and is rung out (step 6): either it stayed (dmg 28) or it came back (outs 1, dmg 0).
        kb = 7 + (before["dmg"] + 20) * 11 / 64
        assert a["face"] == 1 and d["last_dmg"] == 20 and d["last_kb"] == kb, (before, d)
        assert d["dmg"] == before["dmg"] + 20 or (d["outs"] == 1 and d["dmg"] == 0), (before, d)
        print(f"S-F2: sheavy: dmg {before['dmg'] + 20:g}, last_kb {d['last_kb']:g} (7 + 11/64 * {before['dmg'] + 20:g}); dummy outs {d['outs']:g}")
        r.save_screenshot(item, str(SHOTS / "s_f2_after.png"))
        if d["outs"]:  # it flew off the ledge: a fresh match (the dummy at spawn 2 again) for what follows
            r.click(item, "vs:dummy")
            frames(4)

        # --- S-F9: dodges by keys.
        def dodge(held=None):
            frames(60, lambda a, d: a["act"] == "idle")
            a, _ = frames(20) # the cooldown (48 ticks from the last start)
            x0, face0 = a["x"], a["face"]
            tap("KeyL", "l", held=held)
            acts, inv, shot = set(), 0, False
            for _ in range(40):
                a = fs(1)
                acts.add(a["act"])
                if a["inv"] > 0:
                    inv += 1
                    if not shot and not held:
                        r.save_screenshot(item, str(SHOTS / "s_f9_dodge_inv.png"))
                        shot = True
                if a["act"] == "idle":
                    break
                r.frame(1)
            return a, x0, face0, acts, inv

        a, x0, face0, acts, inv = dodge()
        assert "dodge_spot" in acts and inv >= 5 and a["act"] == "idle" and a["x"] == x0, (acts, inv, a, x0)
        tap("KeyL", "l")
        a, _ = frames(2)
        assert a["act"] == "idle", ("no dodge inside the cooldown", a)
        frames(20)
        tap("KeyL", "l")
        a, _ = frames(2)
        assert a["act"] == "dodge_spot", ("a dodge after the cooldown", a)
        a, x0, face0, acts, inv = dodge("ArrowRight")
        assert "dodge_dir" in acts and 131 <= a["x"] - x0 <= 140 and a["face"] == face0 and inv >= 5, (acts, inv, a["x"] - x0, a["face"], face0)
        print(f"S-F9: spot dodge ({inv} sampled frames invulnerable), cooldown held, directional moved {a['x'] - x0:g}")

        # --- S-F6: 150 %, side K near the edge: a ring-out, then back invulnerable at 0 %.
        walk_to(fs(2)["x"] - 170)
        r.click(item, "dummy:pct")
        a, d = frames(4)
        assert d["dmg"] == 150, d
        outs = d["outs"]
        tap("KeyK", "k", held="ArrowRight")
        a, d = frames(400, lambda a, d: d["act"] == "out")
        assert d["act"] == "out" and d["outs"] == outs + 1 and d["dmg"] == 0, d
        r.keyboard("F2", "F2", True)
        r.keyboard("F2", "F2", False)
        a, d = frames(200, lambda a, d: d["act"] != "out")
        assert d["act"] != "out" and d["inv"] > 0 and d["dmg"] == 0 and d["stocks"] == -1, d
        print(f"S-F6: rang out at 150% (outs {outs:g} -> {d['outs']:g}), back at ({d['x']:g}, {d['y']:g}) with inv {d['inv']:g}")

        # --- S-F2 (step 3): every move by keys, drawn on its own clip; its first active frame shot with
        # F2's circles on. The fighting clips were warmed a view at a time long before this.
        r.click(item, "vs:dummy")
        r.frame(2)
        assert text("look:loaded") == "clips=27/27 robots=3/3", text("look:loaded")
        r.keyboard("F2", "F2", True)
        r.keyboard("F2", "F2", False)
        assert text("overlay:state") == "on"
        MOVES = [("nlight", "KeyJ", None, False), ("dlight", "KeyJ", "ArrowDown", False), ("nheavy", "KeyK", None, False),
                 ("dheavy", "KeyK", "ArrowDown", False), ("slight", "KeyJ", "ArrowRight", False), ("sheavy", "KeyK", "ArrowRight", False),
                 ("nair", "KeyJ", None, True), ("sair", "KeyJ", "ArrowRight", True), ("dair", "KeyJ", "ArrowDown", True),
                 ("shair", "KeyK", "ArrowRight", True), ("dhair", "KeyK", "ArrowDown", True)]
        for name, code, held, air in MOVES:
            frames(240, lambda a, d: a["act"] == "idle")
            if air:
                tap("Space", " ")
                r.frame(4)
            tap(code, code[-1].lower(), held=held)
            shot = None
            for _ in range(60):
                tree = r.dump_tree(item)
                a = fs(1, tree)
                k = a["act_t"] + 1
                if a["act"] == name and any(lo <= k <= hi for lo, hi in WINDOWS[name]):
                    drawn = text("look:state", tree)
                    assert drawn.split()[1] == f"clip={name}", (name, drawn)
                    assert "hit:1" in {x["id"] for x in r.rects()}, (name, "no circles drawn")
                    r.save_screenshot(item, str(SHOTS / f"s_f2_{name}_active.png"))
                    shot = k
                    break
                r.frame(1)
            assert shot, (name, "never reached an active tick", fs(1))
            print(f"S-F2: {name} on its own clip, shot at move tick {shot:g}")
        frames(240, lambda a, d: a["act"] == "idle")
        r.keyboard("F2", "F2", True)
        r.keyboard("F2", "F2", False)

        # --- S-F4: the jet by keys.
        def look_state():
            return dict(p.split("=") for p in text("look:state").split())

        def watch_jet(n, until):
            for _ in range(n):
                r.frame(1)
                a = fs(1)
                if until(a, look_state()):
                    return a
            raise AssertionError(("jet", fs(1), look_state()))

        frames(240, lambda a, d: a["act"] == "idle")
        tap("Space", " ")
        r.frame(3)
        r.keyboard("KeyK", "k", True)
        a = watch_jet(4, lambda a, ls: a["act"] == "jet")
        assert a["fuel"] == 96 and a["jet_pow"] == 3 and look_state()["flame"] == "3" and look_state()["clip"] == "jet", (a, look_state())
        r.save_screenshot(item, str(SHOTS / "jet_ignite.png"))
        a = watch_jet(10, lambda a, ls: a["act"] == "jet_hover")
        fuel0 = a["fuel"]
        r.frame(4)
        a, ls = fs(1), look_state()
        assert a["act"] == "jet_hover" and a["fuel"] < fuel0 and ls["flame"] == "1" and ls["clip"] == "jet_hover", (a, ls)
        r.save_screenshot(item, str(SHOTS / "jet_hover.png"))
        r.keyboard("ArrowUp", "ArrowUp", True)
        a = watch_jet(4, lambda a, ls: ls["flame"] == "2")
        assert a["act"] == "jet_hover" and a["jet_pow"] == 2, a
        r.keyboard("ArrowUp", "ArrowUp", False)
        r.keyboard("ArrowLeft", "ArrowLeft", True)
        a = watch_jet(4, lambda a, ls: a["act"] == "jet_thrust")
        r.frame(3)
        a, ls = fs(1), look_state()
        assert a["act"] == "jet_thrust" and a["face"] == -1 and ls["flame"] == "2" and ls["clip"] == "jet_thrust", (a, ls)
        r.save_screenshot(item, str(SHOTS / "jet_thrust.png"))
        r.keyboard("ArrowLeft", "ArrowLeft", False)
        r.keyboard("KeyK", "k", False)
        a = watch_jet(3, lambda a, ls: a["act"] == "jet_cut")
        r.frame(1)
        assert fs(1)["act"] == "jet_cut" and look_state()["flame"] == "0", (fs(1), look_state())
        r.save_screenshot(item, str(SHOTS / "jet_cut.png"))
        left = a["fuel"]
        a, _ = frames(400, lambda a, d: a["fuel"] == 120 and a["y"] == 0)
        assert a["fuel"] == 120 and a["y"] == 0, a
        print(f"S-F4: ignite (fuel 96, flame 3), hover (flame 1, fuel falling from {fuel0:g}), climb (flame 2), thrust, cut with {left:g} left, refilled to 120 on landing")
        frames(240, lambda a, d: a["act"] == "idle")

        # --- S-F3: the fight script, twice.
        def fight_run():
            r.click(item, "script:fight")
            r.frame(2)
            assert find(r.dump_tree(item), "hud:script"), "no scripted run"
            for _ in range(120):
                r.frame(10)
                if parse(text("fighter:state"))["tick"] >= FIGHT_TICKS:
                    break
            r.frame(10)
            tree = r.dump_tree(item)
            finals = {i: text(f"fight:state:{i}", tree) for i in (1, 2)}
            checks = {}
            for pair in text("match:checks", tree).split():
                frame, h = pair.split("=")
                if int(frame) <= FIGHT_TICKS:
                    checks[int(frame)] = h
            return finals, checks, parse(text("fighter:state", tree))["tick"]

        first, first_checks, t1 = fight_run()
        second, second_checks, t2 = fight_run()
        assert t1 == t2 == FIGHT_TICKS, (t1, t2)
        assert first == second, (first, second)
        assert first_checks == second_checks and len(first_checks) >= 14, (first_checks, second_checks)
        expect = lua_finals()
        if expect:
            assert first == expect, (first, expect)
        else:
            print("SKIPPED the comparison with P-F13: no lua on PATH")
        print("S-F3: fight script twice, equal; final", first[1], "|", first[2])
        r.click(item, "vs:dummy")
        r.frame(2)

        # --- S-F5: the CPU.
        assert text("mode:label") == "Dummy"
        r.click(item, "mode")
        r.frame(2)
        assert text("mode:label") == "CPU", text("mode:label")
        a, c = fs(1), fs(2)
        assert abs(c["x"] - SPAWN2[0]) < 40 and c["y"] == 0 and c["stocks"] == 3 and c["act"] != "idle", c  # it has set off already
        r.save_screenshot(item, str(SHOTS / "cpu_open.png"))
        # keys idle: the CPU closes in and swings
        attack_shot = False
        for _ in range(300):
            a, c = frames(1)
            if c["act"] in WINDOWS and any(lo <= c["act_t"] + 1 <= hi for lo, hi in WINDOWS[c["act"]]):
                r.save_screenshot(item, str(SHOTS / "cpu_attack.png"))
                attack_shot = c["act"]
                break
        assert attack_shot and a["dmg"] >= 0, "the CPU never swung"
        # the hidden `vs:bots`: two CPUs, until one is knocked off and jets back
        r.click(item, "vs:bots")
        r.frame(2)
        assert text("mode:label") == "CPU vs CPU"
        recover_shot = None
        for _ in range(3000):
            a, c = frames(1)
            for i, g in ((1, a), (2, c)):
                if abs(g["x"]) > 900 and g["act"] in ("jet", "jet_hover", "jet_thrust"):
                    recover_shot = (i, g["act"], g["x"], g["y"])
            if recover_shot:
                r.save_screenshot(item, str(SHOTS / "cpu_recover.png"))
                break
        assert recover_shot, "no CPU was knocked off and jetted back"
        landed = None
        i = recover_shot[0]
        for _ in range(300):
            g = fs(i)
            if g["act"] in ("out", "ko"):
                break
            if g["y"] == 0 and abs(g["x"]) < 900:
                landed = (g["x"], g["fuel"])
                break
            r.frame(1)
        print(f"S-F5: keys idle, the CPU swung ({attack_shot}); CPU vs CPU: fighter {i} offstage at ({recover_shot[2]:g}, {recover_shot[3]:g}) in {recover_shot[1]}, back on the stage at {landed}")
        r.click(item, "vs:bot")
        r.frame(2)
        # the scripted run, keys idle
        r.click(item, "script:cpu")
        r.frame(2)
        for _ in range(200):
            r.frame(10)
            if parse(text("fighter:state"))["tick"] >= 3600:
                break
        r.frame(10)
        tree = r.dump_tree(item)
        assert parse(text("fighter:state", tree))["tick"] == 3600
        finals = {i: text(f"fight:state:{i}", tree) for i in (1, 2)}
        assert fs(1, tree)["stocks"] < 3, finals
        expect = lua_finals("scripts/test_brawl_bot.lua", "P-CPU5 final human vs bot ")
        if expect:
            assert finals == expect, (finals, expect)
        else:
            print("SKIPPED the comparison with P-CPU5: no lua on PATH")
        print("S-F5: script:cpu, 3 600 ticks, equal to P-CPU5:", finals[1], "|", finals[2])
        # played to the end
        r.click(item, "vs:bot")
        r.frame(2)
        over = None
        for _ in range(600):
            r.frame(10)
            node = find(r.dump_tree(item), "hud:ko")
            if node:
                over = node["text"]
                break
        assert over == "CPU wins — press Enter", over
        won_at = parse(text("fighter:state"))["tick"]
        r.save_screenshot(item, str(SHOTS / "cpu_wins.png"))
        tap("Enter", "Enter")
        r.frame(2)
        tree = r.dump_tree(item)
        assert not find(tree, "hud:ko") and fs(1, tree)["stocks"] == 3 and fs(2, tree)["stocks"] == 3, (fs(1, tree), fs(2, tree))
        print(f"S-F5: vs:bot with the keys idle: '{over}' by tick {won_at:g}; Enter restarts")
        r.click(item, "vs:dummy")
        r.frame(2)
        console = r.read_console(item)
        assert not console, console
        print("brawl fight smoke ok")


if __name__ == "__main__":
    main()
