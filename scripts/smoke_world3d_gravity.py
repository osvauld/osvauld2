"""Lunar gravity through the real shell, pixels, staged source edits and sealed persistence."""
from pathlib import Path
import tempfile

from osvauld.scene3d import assert_3d_pixels
from osvauld.session import Session, shell_binary

SOURCE = '''
local scene = gfx.scene3d({
    camera={eye={8,8,12},target={0,3,0}},
    objects={
        {id="ground",position={0,-0.5,0},scale={12,1,12},color="#6d7078"},
        {id="probe",position={0,5,0},scale={0.8,0.8,0.8},color="#ffcb62"},
    },
})
local game = gfx.world3d({id="gravity",scene=scene,gravity={0,-1.62,0},bodies={
    {id="ground",position={0,-0.5,0},box={12,1,12}},
    {id="probe",position={0,5,0},box={0.8,0.8,0.8},dynamic=true},
}})
local running = false
return function()
    return ui.col({full=true,stretch=true,gap=8,
        ui.scene3d({id="gravity-view",grow=true,scene=game:scene(nil,{running=running}),on_click=function() end}),
        ui.row({gap=8,
            ui.button({id="release",h=40,px=16,"Release",on_click=function() running=true end}),
            ui.button({id="pause",h=40,px=16,"Pause",on_click=function() running=false end}),
            ui.button({id="reset",h=40,px=16,"Reset",on_click=function()
                running=false
                game:reset("probe")
            end}),
        }),
    })
end
'''


def state(rpc, item):
    tree = rpc.dump_tree(item)
    assert "worlds3d" in tree, (tree, rpc.read_console(item))
    return tree["worlds3d"]["gravity"]


def probe(world):
    return next(e["resolved"] for e in world["entities"] if e["authored"]["id"] == "probe")


def edit(rpc, item, old, new):
    source = rpc.read_file_versioned(item, "main.lua")
    return rpc.edit_file(item, "main.lua", source["revision"], [{"old_text": old, "new_text": new}])


def fall(rpc, item, gravity):
    rpc.click_at(*rpc.centre_of("reset"))
    rpc.click_at(*rpc.centre_of("release"))
    before = state(rpc, item)
    rpc.frame(30)
    after = state(rpc, item)
    assert after["tick"] - before["tick"] == 60, (before, after)
    expected = probe(before)["velocity"][1] + gravity / 2
    assert abs(probe(after)["velocity"][1] - expected) < 0.01, (after, expected)
    return after


with tempfile.TemporaryDirectory(prefix="osvauld-gravity-") as tmp:
    data = str(Path(tmp) / "store")
    with Session(shell_binary=shell_binary(), data_dir=data, offscreen=(900, 700)) as s:
        rpc = s.rpc
        rpc.signup("gravity", "gravity smoke passphrase")
        ws = rpc.create_workspace("Lunar gravity")
        item = rpc.create_item(ws["id"], "Gravity probe", "app")["id"]
        rpc.write_file(item, "main.lua", SOURCE)
        rpc.open_item(item)
        ready = state(rpc, item)
        assert abs(ready["gravity"][1] + 1.62) < 1e-6, ready
        rpc.frame(60)
        assert state(rpc, item) == ready, "paused worlds cannot spend time"
        assert_3d_pixels(rpc, item, "gravity-view", "probe")
        assert state(rpc, item) == ready, "pixel observations cannot move physics"
        moon = fall(rpc, item, -1.62)
        rpc.screenshot(item)
        rpc.screenshot(item, width=1100, height=800, scale=1)
        rpc.save_screenshot(item, Path(__file__).resolve().parent.parent / "shots" / "world3d-gravity.png")
        assert state(rpc, item) == moon, "captures cannot advance native physics"
        rpc.click_at(*rpc.centre_of("pause"))
        paused = state(rpc, item)
        for invalid in ["gravity={0,math.huge,0}", "gravity={0,-10001,0}", "gravity={0,'-1.62',0}"]:
            result = edit(rpc, item, "gravity={0,-1.62,0}", invalid)
            assert result["persisted"] and "failed" in result["activation"], result
            assert state(rpc, item) == paused, "rejected edit mutated a live world"
            # App edits persist even if activation fails; repair the saved candidate explicitly.
            repaired = edit(rpc, item, invalid, "gravity={0,-1.62,0}")
            assert repaired["activation"] == "activated", repaired
            assert rpc.read_file_versioned(item, "main.lua")["content"] == SOURCE
        result = edit(rpc, item, "gravity={0,-1.62,0}", "gravity={0,0,0}")
        assert result["persisted"] and result["activation"] == "activated", result
        changed = state(rpc, item)
        assert changed["tick"] == paused["tick"] and probe(changed) == probe(paused)
        fall(rpc, item, 0)
        result = edit(rpc, item, ",gravity={0,0,0}", "")
        assert result["activation"] == "activated", result
        fall(rpc, item, -9.81)
        result = edit(rpc, item, "scene=scene,bodies", "scene=scene,gravity={0,-1.62,0},bodies")
        assert result["activation"] == "activated", result
        rpc.lock()
    with Session(shell_binary=shell_binary(), data_dir=data, offscreen=(900, 700)) as s:
        rpc = s.rpc
        rpc.unlock("gravity", "gravity smoke passphrase")
        rpc.open_item(item)
        assert abs(state(rpc, item)["gravity"][1] + 1.62) < 1e-6
        fall(rpc, item, -1.62)
        assert not rpc.read_console(item), rpc.read_console(item)
    print("3D gravity ok: lunar/default/zero, pixels, pause, captures, atomic edits and restart")
