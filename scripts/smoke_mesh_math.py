"""Lua-authored shared meshes: raw inspection, triangle picking, regeneration and captures."""

import math
from pathlib import Path

from osvauld.session import Session, shell_binary

ROOT = Path(__file__).resolve().parent.parent


def nodes(node):
    yield node
    for child in node.get("children", []):
        yield from nodes(child)


def scene(rpc, item):
    return next(n["scene3d"] for n in nodes(rpc.dump_tree(item)) if "scene3d" in n)


def project(camera, world, rect):
    """Aim using inspected camera/geometry and the resolved viewport, never guessed pixels."""
    def dot(a, b):
        return sum(x * y for x, y in zip(a, b))

    def unit(a):
        length = math.sqrt(dot(a, a))
        return [v / length for v in a]

    def cross(a, b):
        return [a[1]*b[2]-a[2]*b[1], a[2]*b[0]-a[0]*b[2], a[0]*b[1]-a[1]*b[0]]

    eye = camera["eye"]
    forward = unit([t-e for t, e in zip(camera["target"], eye)])
    right = unit(cross(forward, camera["up"]))
    up = cross(right, forward)
    delta = [p-e for p, e in zip(world, eye)]
    extent = dot(delta, forward) * math.tan(camera["fov_y_radians"] / 2)
    x = dot(delta, right) / (extent * rect["w"] / rect["h"])
    y = dot(delta, up) / extent
    return rect["x"] + (x+1)*rect["w"]/2, rect["y"] + (1-y)*rect["h"]/2


with Session(shell_binary=shell_binary(), offscreen=(900, 700)) as s:
    rpc = s.rpc
    rpc.signup("mesh", "mesh passphrase")
    ws = rpc.create_workspace("Mesh math")
    item = rpc.create_item(ws["id"], "Mesh Math", "app")["id"]
    rpc.upload_folder(item, ROOT / "demo_apps" / "mesh_math")
    rpc.open_item(item)
    before = scene(rpc, item)
    left, right = before["objects"]
    assert left["mesh"] == right["mesh"] == "triangles"
    assert left["mesh_resource"] == right["mesh_resource"]
    assert (left["vertices"], left["triangles"], left["mesh_bytes"]) == (475, 864, 16584)
    viewport = next(r for r in rpc.rects() if r["id"] == "mesh-view")
    rpc.click_at(*project(before["camera"], left["position"], viewport))
    status = next(n["text"] for n in nodes(rpc.dump_tree(item)) if n.get("id", "").endswith("mesh-status"))
    assert "Selected: coral-wave" in status, status
    rpc.click_at(*rpc.centre_of("wave-flat"))
    flat = scene(rpc, item)["objects"][0]
    assert flat["mesh_resource"] != left["mesh_resource"]
    assert flat["local_bounds"]["min"][1] == flat["local_bounds"]["max"][1] == 0
    source = rpc.read_file_versioned(item, "main.lua")
    result = rpc.edit_file(item, "main.lua", source["revision"], [{
        "old_text": "= 0.6, 0.5, 0.4, 13", "new_text": "= 1.2, 0.5, 0.4, 13",
    }])
    assert result["persisted"] and result["activation"] == "activated", result
    edited = scene(rpc, item)["objects"][0]
    assert edited["local_bounds"]["max"][1] > left["local_bounds"]["max"][1]
    shots = ROOT / "shots"
    shots.mkdir(exist_ok=True)
    rpc.save_screenshot(item, shots / "mesh-math.png")
    size = rpc.save_screenshot(item, shots / "mesh-math-custom.png", width=1000, height=700, scale=1)
    assert size == {"width_px": 1000, "height_px": 700}, size
    # An empty scene must clear old 3D pixels, exactly as omitting the viewport would.
    source = rpc.read_file_versioned(item, "main.lua")
    result = rpc.edit_file(item, "main.lua", source["revision"], [{
        "old_text": 'ipairs({ "coral-wave", "blue-wave" })', "new_text": "ipairs({})",
    }])
    assert result["activation"] == "activated", result
    assert scene(rpc, item)["objects"] == []
    empty = rpc.screenshot(item, width=1000, height=700, scale=1)
    source = rpc.read_file_versioned(item, "main.lua")
    result = rpc.edit_file(item, "main.lua", source["revision"], [
        {"old_text": "ui.scene3d({", "new_text": "ui.col({"},
        {"old_text": 'id = "mesh-view", scene = scene,', "new_text": 'id = "mesh-view",'},
    ])
    assert result["activation"] == "activated", result
    absent = rpc.screenshot(item, width=1000, height=700, scale=1)
    assert empty["png_base64"] == absent["png_base64"], "empty scene left stale GPU pixels"
    assert rpc.read_console(item) == [], rpc.read_console(item)

print("mesh math smoke ok: shared geometry, raw bounds/counts, real triangle picking, edits and captures")
