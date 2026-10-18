"""Lua-authored shared meshes: raw inspection, triangle picking, regeneration and captures."""

from pathlib import Path

from osvauld.scene3d import assert_3d_pixels, project
from osvauld.session import Session, shell_binary

ROOT = Path(__file__).resolve().parent.parent


def nodes(node):
    yield node
    for child in node.get("children", []):
        yield from nodes(child)


def scene(rpc, item):
    return next(n["scene3d"] for n in nodes(rpc.dump_tree(item)) if "scene3d" in n)


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
    # The mesh is drawn, not only inspectable: where the coral wave projects differs from the
    # viewport's empty corner. Picking alone passes without a single 3D pixel on screen.
    assert_3d_pixels(rpc, item, "mesh-view", "coral-wave")
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
