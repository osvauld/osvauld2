"""Jetpack gun stats over the real path (design/jetpack-militia.md step 4, tests 4 and 6): the HUD ammo
label counts down as the player fires, RELOADING shows when the mag is empty, and the mag refills
after the gun's reload time. Labels are read from the tree (text nodes are not in `rects`)."""
import shutil
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from osvauld.session import Session, shell_binary


def find(tree, identity):
    if tree.get("id") == identity:
        return tree
    for child in tree.get("children", []):
        found = find(child, identity)
        if found:
            return found


def run(size):
    with Session(shell_binary=shell_binary(), offscreen=size) as s:
        r = s.rpc
        r.signup("pilot", "jetpack combat passphrase")
        ws = r.create_workspace("Jetpack combat")
        item = r.create_item(ws["id"], "Jetpack Arena", "app")["id"]
        r.upload_folder(item, Path(__file__).resolve().parent.parent / "demo_apps/jetpack")
        r.open_item(item)
        r.frame(2)

        def label(identity):
            node = find(r.dump_tree(item), identity)
            return node["text"] if node else None

        def ammo():
            a, m = label("hud:ammo").split("/")
            return int(a), int(m)

        assert not r.read_console(item), r.read_console(item)
        # The baked rifle: 30 rounds, 0.12 s apart, a 1.9 s reload.
        assert label("hud:ammo") == "30/30", label("hud:ammo")
        assert label("hud:reload") is None
        r.click_at(*r.centre_of("match:start"))
        r.frame(2)
        assert label("match:state") == "Playing"
        assert label("hud:ammo") == "30/30"

        r.keyboard("KeyF", "f", True)
        r.frame(30)  # 0.5 s of fire: the first shot plus one per 0.12 s
        now, mag = ammo()
        assert mag == 30 and 22 <= now <= 28, ("ammo after half a second", now)
        assert label("hud:reload") is None
        seen = False
        for _ in range(60):  # 30 rounds take 3.5 s; the mag runs dry well inside 6 s
            r.frame(6)
            if label("hud:reload"):
                seen = True
                break
        assert seen, ("never reloaded", label("hud:ammo"))
        assert label("hud:reload") == "RELOADING"
        assert ammo()[0] == 0, label("hud:ammo")
        r.keyboard("KeyF", "f", False)
        shots_before = int(label("hud:fired").split()[-1])
        r.frame(60)  # 1 s: still reloading, and a released trigger fires nothing
        assert label("hud:reload") == "RELOADING", "reload ended early"
        r.frame(70)  # past 1.9 s in all
        assert label("hud:reload") is None, "reload never ended"
        assert label("hud:ammo") == "30/30", label("hud:ammo")
        assert int(label("hud:fired").split()[-1]) == shots_before
        assert not r.read_console(item), r.read_console(item)


def run_mouse(size, gun):
    """Mouse fire and aim (design/jetpack-militia.md step 7, 2026-10-27 note): the left button held on the playfield fires
    (an auto rifle keeps firing while held, a semi pistol once per click); a touch off the sticks does not; a click on a
    button does not; a resting cursor leaves the aim still; the crosshair follows the cursor."""
    with Session(shell_binary=shell_binary(), offscreen=size) as s:
        r = s.rpc
        r.signup("pilot", "jetpack mouse passphrase")
        ws = r.create_workspace("Jetpack mouse")
        if True:  # the baked default is the pistol; seed the loadout so both guns are covered
            seed = Path(tempfile.mkdtemp(prefix="jetpack-loadout-"))
            (seed / "manifest.osv").write_text('app "Seed loadout" {}\n')
            (seed / "main.lua").write_text(
                "local a = asset:open('jetpack-loadout')\n"
                f"a:set({{ 'loadout' }}, doc.map({{ player = '{gun}', bot = '{gun}' }}))\n"
                "return function() return ui.col({ full = true }) end\n")
            seeder = r.create_item(ws["id"], "Seed", "app")["id"]
            r.upload_folder(seeder, seed)
            r.open_item(seeder)
            r.frame(3)
            shutil.rmtree(seed)
        item = r.create_item(ws["id"], "Jetpack Arena", "app")["id"]
        r.upload_folder(item, Path(__file__).resolve().parent.parent / "demo_apps/jetpack")
        r.open_item(item)
        r.frame(2)

        def label(identity):
            node = find(r.dump_tree(item), identity)
            return node["text"] if node else None

        def rect(identity):
            return next((e for e in r.rects() if e["id"] == identity), None)

        def fired():
            return int(label("hud:fired").split()[-1])

        def aim():
            return float(label("aim:angle"))

        r.click_at(*r.centre_of("match:start"))
        r.frame(90)  # the soldier settles on the floor
        assert label("match:state") == "Playing"
        field = rect("playfield")
        target = (field["x"] + field["w"] * 0.72, field["y"] + field["h"] * 0.3)

        # A resting cursor: the crosshair is there, the aim stands still while the camera eases.
        def cross_node():
            return find(r.dump_tree(item), "crosshair")

        r.move_to(*target)
        r.frame(3)
        assert cross_node(), "the crosshair follows the cursor on the playfield"
        r.frame(90)
        first = aim()
        r.frame(30)
        r.frame(30)
        second = aim()
        assert abs(second - first) < 0.0175, ("aim drifts under a resting cursor", first, second)
        assert abs(first) < 1.5, ("aims toward a cursor up and to the right", first)
        assert fired() == 0, "moving the mouse fires nothing"

        # Buttons and touches off the sticks fire nothing.
        for button in ("button:reload", "button:swap", "button:zoom"):
            r.click_at(*r.centre_of(button))
            r.frame(20)
        assert fired() == 0, ("a click on a button fired", fired())
        r.move_to(*target)
        r.frame(2)
        r.touch(7, "start", *target)
        r.frame(25)
        r.touch(7, "move", target[0] - 40, target[1] + 20)
        r.frame(10)
        r.touch(7, "end", target[0] - 40, target[1] + 20)
        r.frame(10)
        assert fired() == 0, ("a touch on the playfield fired", fired())
        r.move_to(*target)
        r.frame(2)

        if gun == "rifle":
            ammo0 = int(label("hud:ammo").split("/")[0])
            r.press()
            r.frame(12)
            a1 = int(label("hud:ammo").split("/")[0])
            assert a1 < ammo0 and fired() >= 1, ("the held button fires", ammo0, a1)
            r.frame(30)
            a2 = int(label("hud:ammo").split("/")[0])
            assert a2 < a1 - 2, ("an auto gun keeps firing while held", a1, a2)
            r.release()
            r.frame(4)
            f = fired()
            r.frame(40)
            assert fired() == f, "releasing stops the fire"
            before = fired()
            r.press()
            r.release()  # a click faster than a world step still fires
            r.frame(6)
            assert fired() > before, "a quick click fires"
        else:
            r.press()
            r.frame(90)  # held for 1.5 s: a semi gun fires once
            assert fired() == 1, ("a semi gun fires once per click", fired())
            r.release()
            r.frame(20)
            for n in (2, 3):
                r.press()
                r.frame(3)
                r.release()
                r.frame(20)
                assert fired() == n, ("one shot per click", n, fired())
        assert not r.read_console(item), r.read_console(item)


if __name__ == "__main__":
    run((960, 540))
    run_mouse((960, 540), "rifle")
    run_mouse((960, 540), "pistol")
    print("jetpack combat smoke ok")
