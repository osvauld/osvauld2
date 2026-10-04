"""Guide-only Tilt Maze acceptance through a fresh offscreen shell and real pointer input."""
import json
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from osvauld.scene3d import assert_3d_pixels
from osvauld.session import Session, build_shell, shell_binary

ROOT = Path(__file__).resolve().parent.parent
SHOTS = ROOT / "shots"


def node(tree, el_id):
    if isinstance(tree, dict):
        if tree.get("id") == el_id:
            return tree
        for child in tree.get("children", []):
            found = node(child, el_id)
            if found:
                return found
    return None


def text(tree, el_id):
    found = node(tree, el_id)
    assert found, f"missing UI {el_id}"
    return found.get("text")


def world(tree):
    return tree["worlds3d"]["tilt-maze"]


def body(tree, body_id="ball"):
    return next(e for e in world(tree)["entities"] if e["authored"]["id"] == body_id)


CAPTURE_REPRO = '''
local scene = gfx.scene3d({
    camera = {eye={0,8,8}, target={0,0,0}, fov_y=48, near=0.1, far=50},
    objects = {
        {id="floor", position={0,-0.5,0}, scale={8,1,8}, color="#58728b"},
        {id="ball", position={-2.6,0.38,-2.7}, scale={0.56,0.56,0.56}, color="#ffc857"},
    },
})
local game = gfx.world3d({id="tilt-maze", scene=scene, bodies={
    {id="floor", position={0,-0.5,0}, box={8,1,8}},
    {id="ball", position={-2.6,0.38,-2.7}, sphere=0.28, dynamic=true},
    {id="checkpoint", position={2.6,0.05,0}, box={1.2,1.8,1.2}, sensor=true},
}})
local title = "Capture repro"
local running = true
return function()
    return ui.col({full=true, stretch=true,
        ui.text({id="title", title}),
        ui.scene3d({id="maze-view", grow=true, scene=game:scene(nil, {running=running}),
            on_zone=function(e) end, on_click=function() end}),
        ui.button({id="retry", h=40, "Retry checkpoint", on_click=function()
            game:set("floor", {pos={0,-0.5,0}, rotation={0,0,0,1}})
            game:set("checkpoint", {pos={2.6,0.05,0}, rotation={0,0,0,1}})
            game:reset("ball")
            game:set("ball", {pos={2.6,0.38,0}})
        end}),
        ui.button({id="pause", h=40, "Pause", on_click=function() running=false end}),
    })
end
'''


def capture_repro(rpc, item):
    rpc.write_file(item, "main.lua", CAPTURE_REPRO)
    rpc.open_item(item)
    assert_3d_pixels(rpc, item, "maze-view", "ball")
    # First independently prove a surgical reload in a paused native world.
    rpc.frame(20)
    rpc.click_at(*rpc.centre_of("pause"))
    before_reload = rpc.dump_tree(item)
    src = rpc.read_file_versioned(item, "main.lua")
    edited = rpc.edit_file(item, "main.lua", src["revision"], [
        {"old_text": 'local title = "Capture repro"', "new_text": 'local title = "Capture repro / reloaded"'},
    ])
    after_reload = rpc.dump_tree(item)
    assert edited["persisted"] and edited["activation"] == "activated"
    assert world(before_reload) == world(after_reload)
    (SHOTS / "tilt-maze-repro-reload.json").write_text(json.dumps({
        "before": before_reload, "after": after_reload, "edit": edited,
    }, indent=2))
    # Locals reset, so the reloaded repro runs again. No time-producing requests between
    # the two snapshots except Screenshot, which the guide promises is observational.
    for attempt in range(12):
        rpc.click_at(*rpc.centre_of("retry"))
        rpc.frame(8)
        clock = rpc.frame(0)["clock"]
        before = rpc.dump_tree(item)

        def check_observation(operation):
            after = rpc.dump_tree(item)
            if world(before) != world(after):
                after_clock = rpc.frame(0)["clock"]
                (SHOTS / "tilt-maze-minimal-mutation.json").write_text(json.dumps({
                    "attempt": attempt, "operation": operation, "clock_before": clock,
                    "clock_after": after_clock, "before": before, "after": after,
                }, indent=2))
                print("minimal repro:", operation, "native ticks", world(before)["tick"], "->",
                      world(after)["tick"], "clock", clock, "->", after_clock, flush=True)
                raise AssertionError("Observational bridge requests changed native state without driven time")

        for _ in range(3):
            check_observation("DumpTree")
        rpc.rects()
        rpc.read_console(item)
        check_observation("Rects/ReadConsole/DumpTree")
        rpc.save_screenshot(item, SHOTS / "tilt-maze-minimal-repro.png")
        check_observation("Screenshot/DumpTree")
    print("minimal capture repro did not reproduce")


def main():
    build_shell()
    SHOTS.mkdir(exist_ok=True)
    with Session(shell_binary=shell_binary(), offscreen=(900, 700)) as s:
        rpc = s.rpc
        rpc.signup("tilt-maker", "throwaway maze passphrase")
        ws = rpc.create_workspace("Tilt proof")
        item = rpc.create_item(ws["id"], "Tilt Maze", "app")["id"]
        if "--capture-repro" in sys.argv:
            capture_repro(rpc, item)
            return
        # Initial upload is intentionally the first red assertion before any app exists.
        rpc.upload_folder(item, ROOT / "demo_apps/tilt_maze")
        rpc.open_item(item)
        # Rects exposes hit regions, not passive layout. Instrument only the uploaded copy
        # with a no-op click handler so the pixel probe gets the real viewport, not a guess.
        src = rpc.read_file_versioned(item, "main.lua")
        activated = rpc.edit_file(item, "main.lua", src["revision"], [{
            "old_text": "ui.scene3d({", "new_text": "ui.scene3d({ on_click=function() end,",
        }])
        assert activated["activation"] == "activated", activated

        def snapshot():
            return rpc.dump_tree(item)

        def click(el_id):
            under = rpc.click_at(*rpc.centre_of(el_id))
            assert el_id in [r["id"] for r in under], under

        def shot(name, **size):
            before = world(snapshot())
            rpc.save_screenshot(item, SHOTS / f"tilt-maze-{name}.png", **size)
            after = world(snapshot())
            if after != before:
                path = SHOTS / f"tilt-maze-{name}-mutation.json"
                path.write_text(json.dumps({"before": before, "after": after}, indent=2))
                print("capture difference:", json.dumps({"before": before, "after": after}), flush=True)
            assert after == before, f"capture mutated native world: {name}"

        ready = snapshot()
        assert text(ready, "status") == "Ready"
        assert len(world(ready)["entities"]) >= 10
        assert_3d_pixels(rpc, item, "maze-view", "ball")
        assert world(snapshot()) == world(ready), "pixel probe must not spend physics time"
        shot("ready")
        if "--pixels-only" in sys.argv:
            print("tilt maze pixel gate ok (not full gameplay acceptance)")
            return
        click("release")
        assert text(snapshot(), "status") == "Playing"
        rpc.frame(20)
        click("south")
        rpc.frame(20)
        moving = snapshot()
        assert sum(v*v for v in body(moving)["resolved"]["velocity"]) > 0.01
        click("pause")
        paused = world(snapshot())
        rpc.advance(45)
        rpc.frame(30)
        assert world(snapshot()) == paused, "paused world moved"
        shot("paused")
        click("pause")
        resumed = snapshot()
        assert world(resumed)["tick"] - paused["tick"] <= 8, "hidden-time catch-up"
        assert world(resumed)["dropped_seconds"] == paused["dropped_seconds"]

        # Observations and both capture paths must not tick, move or manufacture moments.
        before = world(snapshot())
        rpc.rects()
        rpc.read_console(item)
        rpc.read_file_versioned(item, "main.lua")
        assert world(snapshot()) == before
        shot("live")
        shot("custom", width=1000, height=760, scale=1)
        click("level")

        # The driving helpers below use native board-local positions, never commands or eval.
        def local_position(tree):
            p = body(tree)["resolved"]["position"]
            q = body(tree, "floor")["resolved"]["rotation"]
            # Rotate by conjugate q: v + 2*w*(u cross v) + 2*(u cross (u cross v)).
            u = [-q[0], -q[1], -q[2]]
            def cross(a, b):
                return [a[1]*b[2]-a[2]*b[1], a[2]*b[0]-a[0]*b[2], a[0]*b[1]-a[1]*b[0]]
            c = cross(u, p)
            cc = cross(u, c)
            return [p[i] + 2*q[3]*c[i] + 2*cc[i] for i in range(3)]

        trace = []

        def drive(control, axis, target, direction, limit=360):
            click(control)
            for _ in range(limit // 8):
                rpc.frame(8)
                tree = snapshot()
                p = local_position(tree)
                trace.append({"control": control, "local": p, "tick": world(tree)["tick"],
                              "zones": body(tree)["zones"]})
                if direction * (p[axis] - target) >= 0:
                    click("level")
                    print("waypoint:", control, p, "tick", world(tree)["tick"], flush=True)
                    return tree
                assert text(tree, "status") == "Playing", (control, p, text(tree, "status"))
            raise AssertionError(("did not reach", control, target, local_position(snapshot())))

        def until_status(expected, limit=180):
            for _ in range(limit // 8):
                rpc.frame(8)
                tree = snapshot()
                trace.append({"control": "until-" + expected, "local": local_position(tree),
                              "tick": world(tree)["tick"], "zones": body(tree)["zones"]})
                if text(tree, "status") == expected:
                    return tree
            raise AssertionError((expected, snapshot()))

        def events(tree):
            found = re.findall(r"([\w-]+): (enter|leave) @(\d+)", text(tree, "events"))
            assert found
            ticks = [int(tick) for _, _, tick in found]
            assert ticks == sorted(ticks) and 0 < ticks[0] <= ticks[-1] <= world(tree)["tick"]
            return {(zone, phase): int(tick) for zone, phase, tick in found}

        def finish_from_checkpoint():
            drive("west", 0, -2.6, -1)
            click("south")
            won = until_status("Won", 360)
            assert "goal" in body(won)["zones"], won
            assert "checkpoint: enter @" in text(won, "events")
            assert "goal: enter @" in text(won, "events")
            assert world(won)["dropped_zone_events"] == 0
            ticks = events(won)
            assert ticks[("checkpoint", "enter")] < ticks[("checkpoint", "leave")] < ticks[("goal", "enter")]
            return won

        drive("south", 2, -1.7, 1)
        # A straight south shortcut is physically stopped by the first staggered wall.
        click("south")
        rpc.frame(100)
        blocked = snapshot()
        blocked_p = local_position(blocked)
        assert -1.65 < blocked_p[2] < -1.3 and blocked_p[0] < -2, blocked_p
        assert text(blocked, "status") == "Playing"
        click("level")
        drive("east", 0, 2.6, 1)
        checkpoint = drive("south", 2, 0, 1)
        assert "checkpoint" in body(checkpoint)["zones"], checkpoint
        assert text(snapshot(), "progress") == "Checkpoint saved"
        assert events(checkpoint)[("checkpoint", "enter")] <= world(checkpoint)["tick"]
        assert local_position(checkpoint)[0] > 1.5 and local_position(checkpoint)[2] > -1
        shot("checkpoint")
        if "--capture-probe" in sys.argv:
            click("retry")
            rpc.frame(8)
            for i in range(10):
                shot(f"probe-{i}")
            print("checkpoint capture probe ok", flush=True)
            return
        # The second wall stops continuing south at the checkpoint; the west gap is necessary.
        click("south")
        rpc.frame(100)
        blocked_two = snapshot()
        blocked_p = local_position(blocked_two)
        assert 0.55 < blocked_p[2] < 0.95 and blocked_p[0] > 2, blocked_p
        click("level")
        won = finish_from_checkpoint()
        assert local_position(won)[0] < -1.5 and local_position(won)[2] > 1.2
        shot("won")
        frozen = world(snapshot())
        rpc.frame(60)
        assert world(snapshot()) == frozen
        assert body(won)["resolved"]["position"] != body(ready)["resolved"]["position"]

        # New run discards checkpoint; driving east along the start row enters red hazard.
        click("release")
        assert text(snapshot(), "progress") == "Find the checkpoint"
        click("east")
        lost = until_status("Lost", 360)
        assert "hazard" in body(lost)["zones"], lost
        assert "hazard: enter @" in text(lost, "events")
        assert events(lost)[("hazard", "enter")] <= world(lost)["tick"]
        shot("lost")
        frozen = world(snapshot())
        rpc.frame(60)
        assert world(snapshot()) == frozen
        click("retry")
        assert text(snapshot(), "status") == "Playing"
        assert text(snapshot(), "attempt") == "Attempt 3"
        drive("south", 2, -1.7, 1)
        drive("east", 0, 2.6, 1)
        drive("south", 2, 0, 1)
        finish_from_checkpoint()
        shot("retry-won")

        # Retry from saved checkpoint is a real progression mechanic, not just a UI flag.
        click("retry")
        p = local_position(snapshot())
        assert abs(p[0] - 2.6) < 0.1 and abs(p[2]) < 0.1, p
        rpc.frame(8)
        shot("checkpoint-retry")
        click("pause")
        before = snapshot()
        src = rpc.read_file_versioned(item, "main.lua")
        result = rpc.edit_file(item, "main.lua", src["revision"], [
            {"old_text": 'local title = "Tilt Maze"', "new_text": 'local title = "Tilt Maze / reloaded"'},
        ])
        assert result["persisted"] and result["activation"] == "activated", result
        after = snapshot()
        assert world(after) == world(before), "accepted reload changed native poses/clock"
        assert text(after, "title") == "Tilt Maze / reloaded"
        assert text(after, "status") == "Ready"  # Lua module locals intentionally reset.
        assert text(after, "attempt") == "Attempt 0"
        assert text(after, "progress") == "Find the checkpoint"
        # Body identity is public authored IDs plus retained state; private native handles aren't exposed.
        # gfx.mesh runs again in the new VM, so its resource ID is intentionally not retained.
        before_objects = node(before, "maze-view")["scene3d"]["objects"]
        after_objects = node(after, "maze-view")["scene3d"]["objects"]
        assert [o["id"] for o in after_objects] == [o["id"] for o in before_objects]
        for old, new in zip(before_objects, after_objects):
            assert {k: v for k, v in old.items() if k != "mesh"} == {k: v for k, v in new.items() if k != "mesh"}
        shot("reloaded")
        assert rpc.read_console(item) == [], rpc.read_console(item)
        (SHOTS / "tilt-maze-evidence.json").write_text(json.dumps({
            "ready": ready, "moving": moving, "blocked_one": blocked, "blocked_two": blocked_two,
            "checkpoint": checkpoint, "won": won, "lost": lost,
            "before_reload": before, "after_reload": after, "edit": result, "trace": trace,
        }, indent=2))
    print("tilt maze smoke ok: win, loss, retry, checkpoint, pause, capture and reload")


if __name__ == "__main__":
    main()
