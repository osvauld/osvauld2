"""Real Lua sphere, native physics, raw inspection, reload safety and capture exclusion."""
from pathlib import Path

from osvauld.scene3d import assert_3d_pixels
from osvauld.session import Session, shell_binary

ROOT = Path(__file__).resolve().parent.parent


def world(rpc, item):
    # Frame(n) paints then advances; resolve its final clock instant before reading.
    rpc.rects()
    return rpc.dump_tree(item)["worlds3d"]["marble-game"]


def marble(state):
    return next(e for e in state["entities"] if e["authored"]["id"] == "marble")


def nodes(tree):
    yield tree
    for child in tree.get("children", []):
        yield from nodes(child)


with Session(shell_binary=shell_binary(), offscreen=(900, 700)) as s:
    rpc = s.rpc
    rpc.signup("marble", "marble passphrase")
    ws = rpc.create_workspace("Marble Gates")
    item = rpc.create_item(ws["id"], "Marble Gates", "app")["id"]
    rpc.upload_folder(item, ROOT / "demo_apps" / "marble_gates")
    rpc.open_item(item)
    initial = world(rpc, item)
    assert marble(initial)["authored"]["shape"] == {"sphere": 0.25}
    scene = next(n["scene3d"] for n in nodes(rpc.dump_tree(item)) if "scene3d" in n)
    visual = next(o for o in scene["objects"] if o["id"] == "marble")
    assert visual["mesh"] == "triangles" and visual["triangles"] == 528, visual
    rpc.frame(20)
    falling = world(rpc, item)
    assert falling["tick"] > initial["tick"]
    assert marble(falling)["resolved"]["position"][1] < 2.8, falling
    assert marble(falling)["resolved"]["velocity"][1] < 0, falling
    scene = next(n["scene3d"] for n in nodes(rpc.dump_tree(item)) if "scene3d" in n)
    visual = next(o for o in scene["objects"] if o["id"] == "marble")
    assert visual["position"] == marble(falling)["resolved"]["position"]
    assert world(rpc, item) == falling, "inspection must not spend virtual time"
    # Both capture paths must be observational, not hidden simulation frames.
    shots = ROOT / "shots"
    shots.mkdir(exist_ok=True)
    assert_3d_pixels(rpc, item, "marble-view", "marble")
    assert_3d_pixels(rpc, item, "marble-view", "platform")
    rpc.save_screenshot(item, shots / "marble-gates-falling.png")
    rpc.save_screenshot(item, shots / "marble-gates-custom.png", width=1000, height=700, scale=1)
    assert world(rpc, item) == falling
    source = rpc.read_file_versioned(item, "main.lua")
    result = rpc.edit_file(item, "main.lua", source["revision"], [{
        "old_text": "sphere = 0.25, position = {0,3,0}",
        "new_text": "sphere = -0.25, position = {0,3,0}",
    }])
    assert result["persisted"] and result["activation"] != "activated", result
    assert world(rpc, item) == falling
    source = rpc.read_file_versioned(item, "main.lua")
    result = rpc.edit_file(item, "main.lua", source["revision"], [{
        "old_text": "sphere = -0.25, position = {0,3,0}",
        "new_text": "sphere = 0.25, position = {0,3,0}",
    }])
    assert result["activation"] == "activated", result
    assert world(rpc, item) == falling
    rpc.frame(600)
    settled = world(rpc, item)
    body = marble(settled)["resolved"]
    assert body["sleeping"] and abs(body["position"][1] - 0.5) < 0.02, settled
    rpc.save_screenshot(item, shots / "marble-gates-settled.png")
    assert world(rpc, item) == settled
    rpc.click_at(*rpc.centre_of("marble-reset"))
    reset = world(rpc, item)
    assert marble(reset)["resolved"]["position"][1] > 2.95, reset
    assert not marble(reset)["resolved"]["sleeping"]
    rpc.frame(8)
    rpc.click(item, "marble-visibility")
    hidden = world(rpc, item)
    rpc.advance(20)
    assert world(rpc, item) == hidden
    rpc.click(item, "marble-visibility")
    resumed = world(rpc, item)
    assert resumed == hidden, (resumed, hidden)
    rpc.frame(5)
    assert world(rpc, item)["tick"] > resumed["tick"]
    assert not any("View error" in line or "handler error" in line for line in rpc.read_console(item)), rpc.read_console(item)
    # A recipe edit changes the reset target, not the retained body's current orientation.
    source = rpc.read_file_versioned(item, "main.lua")
    result = rpc.edit_file(item, "main.lua", source["revision"], [{
        "old_text": 'box = {6,0.5,4}, position = {0,0,0}',
        "new_text": 'box = {6,0.5,4}, position = {0,0,0}, rotation = {0,0,-math.sin(math.rad(10)),math.cos(math.rad(10))}',
    }, {
        "old_text": 'function() game:reset("marble") end',
        "new_text": 'function() game:reset("platform"); game:reset("marble") end',
    }])
    assert result["activation"] == "activated", result
    platform = next(e for e in world(rpc, item)["entities"] if e["authored"]["id"] == "platform")
    assert platform["resolved"]["rotation"] == [0, 0, 0, 1], platform
    assert platform["authored"]["rotation"][2] < -0.17, platform
    rpc.click(item, "marble-reset")
    ramp = world(rpc, item)
    platform = next(e for e in ramp["entities"] if e["authored"]["id"] == "platform")
    assert abs(platform["resolved"]["rotation"][2] + 0.173648) < 0.0001, platform
    scene = next(n["scene3d"] for n in nodes(rpc.dump_tree(item)) if "scene3d" in n)
    visual = next(o for o in scene["objects"] if o["id"] == "platform")
    assert visual["rotation"] == platform["resolved"]["rotation"], visual
    rpc.frame(120)
    rolled = world(rpc, item)
    assert marble(rolled)["resolved"]["position"][0] > 0.5, rolled
    rpc.save_screenshot(item, shots / "marble-gates-ramp.png")
    rpc.save_screenshot(item, shots / "marble-gates-ramp-custom.png", width=1000, height=700, scale=1)
    assert world(rpc, item) == rolled
    # The shipped demo exposes a second Lua-authored level, not a native ramp special case.
    rpc.click(item, "marble-mode")
    rpc.frame(120)
    world(rpc, item)
    demo_ramp = rpc.dump_tree(item)["worlds3d"]["marble-ramp"]
    assert marble(demo_ramp)["resolved"]["position"][0] > 0.5, demo_ramp
    assert world(rpc, item) == rolled, "inactive level must stay paused"
    rpc.save_screenshot(item, shots / "marble-gates-ramp-demo.png")
    assert rpc.dump_tree(item)["worlds3d"]["marble-ramp"] == demo_ramp
    rpc.click(item, "marble-mode")
    source = rpc.read_file_versioned(item, "main.lua")
    result = rpc.edit_file(item, "main.lua", source["revision"], [{
        "old_text": 'rotation = {0,0,-math.sin(math.rad(10)),math.cos(math.rad(10))}',
        "new_text": 'rotation = {0,0,0,0}',
    }])
    assert result["activation"] != "activated", result
    assert world(rpc, item) == rolled, "invalid rotation must preserve all accepted state"
    source = rpc.read_file_versioned(item, "main.lua")
    result = rpc.edit_file(item, "main.lua", source["revision"], [{
        "old_text": 'rotation = {0,0,0,0}',
        "new_text": 'rotation = {0,0,-math.sin(math.rad(10)),math.cos(math.rad(10))}',
    }, {
        "old_text": '{ id = "marble", sphere = 0.25, position = { 0, 3, 0 }, dynamic = true },',
        "new_text": '{ id = "marble", sphere = 0.25, position = { 0, 3, 0 }, dynamic = true },\n'
            '\t\t{ id = "goal", box = {1.5,2,3}, position = {1.5,-0.4,0}, sensor = true },',
    }, {
        "old_text": 'on_zone = function(e) if game == level.world then level.zone(e) end end,',
        "new_text": 'on_zone = function(e) zone_events = zone_events .. e.phase .. ":" .. e.id .. ":" .. e.who .. ":" .. e.tick .. "|"; '
            'if game == level.world then level.zone(e) end end,',
    }, {
        "old_text": 'local visible = true',
        "new_text": 'local visible = true\nlocal zone_events = ""',
    }, {
        "old_text": 'visible and ui.scene3d({',
        "new_text": 'ui.text({ zone_events, id = "marble-zones", color = C.text }),\n\t\tvisible and ui.scene3d({',
    }])
    assert result["activation"] == "activated", result
    rpc.click(item, "marble-mode")
    rpc.click(item, "marble-reset")
    for _ in range(120):
        rpc.frame(1)
        world(rpc, item)
        inside = rpc.dump_tree(item)["worlds3d"]["marble-ramp"]
        if marble(inside)["zones"] == ["goal"]:
            break
    else:
        raise AssertionError(f"marble never entered goal: {inside}")
    before_capture = rpc.read_console(item)
    rpc.save_screenshot(item, shots / "marble-gates-zone.png")
    assert rpc.dump_tree(item)["worlds3d"]["marble-ramp"] == inside
    assert rpc.read_console(item) == before_capture
    rpc.frame(240)
    world(rpc, item)
    outside = rpc.dump_tree(item)["worlds3d"]["marble-ramp"]
    assert marble(outside)["zones"] == [], outside
    event_text = next(n["text"] for n in nodes(rpc.dump_tree(item)) if n.get("id") == "marble-zones")
    events = [event for event in event_text.split("|") if event]
    assert len(events) == 2 and events[0].startswith("enter:goal:marble:") and events[1].startswith("leave:goal:marble:"), events
    assert int(events[0].rsplit(":", 1)[1]) < int(events[1].rsplit(":", 1)[1]), events
    assert outside["dropped_zone_events"] == 0, outside
    source = rpc.read_file_versioned(item, "main.lua")
    result = rpc.edit_file(item, "main.lua", source["revision"], [{
        "old_text": 'visible and ui.scene3d({',
        "new_text": 'ui.button({id="marble-command", on_click=function() '
            'game:set("platform", {rotation={0,0,math.sin(math.rad(10)),math.cos(math.rad(10))}}); '
            'game:set("marble", {pos={1,3,0},velocity={2,0,0},spin={0,90,0}}) end, "Command"}),\n'
            '\t\tui.button({id="marble-bad-command", on_click=function() '
            'assert(not pcall(function() game:set("marble", {pos={8,8,8},spin={0,math.huge,0}}) end)) end, "Bad command"}),\n'
            '\t\tvisible and ui.scene3d({',
    }])
    assert result["activation"] == "activated", result
    before = world(rpc, item)
    rpc.click(item, "marble-command")
    commanded = world(rpc, item)
    body = marble(commanded)["resolved"]
    assert body["position"] == [1, 3, 0] and body["velocity"] == [2, 0, 0], commanded
    assert abs(body["angular_velocity"][1] - 1.5707963) < 0.00001, body
    assert commanded["tick"] == before["tick"]
    assert marble(commanded)["authored"] == marble(before)["authored"]
    platform = next(e for e in commanded["entities"] if e["authored"]["id"] == "platform")
    assert platform["resolved"]["rotation"][2] > 0.17, platform
    scene = next(n["scene3d"] for n in nodes(rpc.dump_tree(item)) if "scene3d" in n)
    assert next(o for o in scene["objects"] if o["id"] == "marble")["position"] == [1, 3, 0]
    rpc.click(item, "marble-bad-command")
    assert world(rpc, item) == commanded, "invalid command must be atomic"
    rpc.click(item, "marble-reset")
    assert marble(world(rpc, item))["resolved"]["position"] == [0, 3, 0]
    source = rpc.read_file_versioned(item, "main.lua")
    result = rpc.edit_file(item, "main.lua", source["revision"], [{
        "old_text": "scene = game:scene(camera, options)",
        "new_text": "scene = game:scene(camera, {running=false})",
    }])
    assert result["activation"] == "activated", result
    paused = world(rpc, item)
    rpc.advance(20)
    rpc.frame(10)
    assert world(rpc, item) == paused, "paused snapshot must render without running physics"
    rpc.save_screenshot(item, shots / "marble-gates-paused.png")
    assert world(rpc, item) == paused
    source = rpc.read_file_versioned(item, "main.lua")
    result = rpc.edit_file(item, "main.lua", source["revision"], [{
        "old_text": "scene = game:scene(camera, {running=false})",
        "new_text": "scene = game:scene(camera, options)",
    }])
    assert result["activation"] == "activated", result
    assert world(rpc, item) == paused, "resume must not catch up paused time"
    rpc.frame(1)
    assert world(rpc, item)["tick"] == paused["tick"] + 2
    print("marble gates ok: drop/reset, pause, reload, ramp, zones, atomic commands and captures")
