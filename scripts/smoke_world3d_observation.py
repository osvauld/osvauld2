"""Driver replies settle shown native worlds; zero-time senses/captures cannot catch up later."""
from pathlib import Path

from osvauld.scene3d import assert_3d_pixels
from osvauld.session import Session, shell_binary

ROOT = Path(__file__).resolve().parent.parent
SOURCE = '''
local scene = gfx.scene3d({
    camera={eye={0,8,8},target={0,0,0}},
    objects={
        {id="floor",position={0,-0.5,0},scale={8,1,8},color="#58728b"},
        {id="ball",position={2.6,0.38,0},scale={0.56,0.56,0.56},color="#ffc857"},
    },
})
local game = gfx.world3d({id="probe",scene=scene,bodies={
    {id="floor",position={0,-0.5,0},box={8,1,8}},
    {id="ball",position={2.6,0.38,0},sphere=0.28,dynamic=true},
    {id="zone",position={2.6,0.05,0},box={1.2,1.8,1.2},sensor=true},
}})
return function()
    return ui.col({full=true,stretch=true,
        ui.scene3d({id="probe-view",grow=true,scene=game:scene(),on_click=function() end}),
        ui.button({id="reset",h=40,"Reset",on_click=function() game:reset("ball") end}),
    })
end
'''

with Session(shell_binary=shell_binary(), offscreen=(900, 700)) as s:
    rpc = s.rpc
    rpc.signup("observation", "throwaway observation passphrase")
    ws = rpc.create_workspace("Observation boundary")
    item = rpc.create_item(ws["id"], "Observation probe", "app")["id"]
    rpc.write_file(item, "main.lua", SOURCE)
    rpc.open_item(item)
    assert_3d_pixels(rpc, item, "probe-view", "ball")

    def state():
        # Deliberately NO Rects here: the first native dump must already be at the driver clock.
        return rpc.dump_tree(item)["worlds3d"]["probe"]

    def unchanged(expected, name):
        actual = state()
        assert actual == expected, f"{name}: ticks {expected['tick']} -> {actual['tick']}: {actual}"

    for _ in range(4):
        rpc.click_at(*rpc.centre_of("reset"))
        rpc.frame(8)
        clock = rpc.frame(0)["clock"]
        before = state()
        assert before["tick"] > 0
        for _ in range(4):
            unchanged(before, "DumpTree")
        rpc.rects()
        rpc.read_console(item)
        unchanged(before, "Rects/ReadConsole")
        rpc.screenshot(item)
        unchanged(before, "live capture")
        rpc.screenshot(item, width=1000, height=760, scale=1)
        unchanged(before, "custom capture")
        assert rpc.frame(0)["clock"] == clock
        unchanged(before, "Frame(0)")
        rpc.frame(1)
        after = state()
        assert after["tick"] == before["tick"] + 2
        unchanged(after, "first dump after Frame(1)")
        rpc.advance(1 / 60)
        advanced = state()
        assert advanced["tick"] == after["tick"] + 2
        unchanged(advanced, "first dump after Advance")
    print("3D observation ok: settled driver replies, repeated senses, live/custom pixels and explicit time")
