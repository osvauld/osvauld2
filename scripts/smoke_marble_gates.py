"""Real Lua sphere, native physics, raw inspection, reload safety and capture exclusion."""
from pathlib import Path

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
    rpc.save_screenshot(item, shots / "marble-gates-falling.png")
    rpc.save_screenshot(item, shots / "marble-gates-custom.png", width=1000, height=700, scale=1)
    assert world(rpc, item) == falling
    source = rpc.read_file_versioned(item, "main.lua")
    result = rpc.edit_file(item, "main.lua", source["revision"], [{
        "old_text": "sphere = 0.25", "new_text": "sphere = -0.25",
    }])
    assert result["persisted"] and result["activation"] != "activated", result
    assert world(rpc, item) == falling
    source = rpc.read_file_versioned(item, "main.lua")
    result = rpc.edit_file(item, "main.lua", source["revision"], [{
        "old_text": "sphere = -0.25", "new_text": "sphere = 0.25",
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
    print("marble gates ok: procedural sphere, fall/settle/reset, pause, reload and captures")
