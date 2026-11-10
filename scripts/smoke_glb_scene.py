"""E2: a GLB binary file loaded by Rust, composed into a scene by Lua (blender-import.md §1)."""
from pathlib import Path
import json
import base64
import math

from osvauld.png import Image
from osvauld.scene3d import assert_3d_pixels, project
from osvauld.session import Session, shell_binary

ROOT = Path(__file__).resolve().parent.parent
APP = ROOT / "demo_apps" / "glb_scene"
FIXTURES = ROOT / "scripts" / "fixtures" / "glb"


def nodes(node):
    yield node
    for child in node.get("children", []):
        yield from nodes(child)


def scene(rpc, item):
    tree = rpc.dump_tree(item)
    return next(n["scene3d"] for n in nodes(tree) if "scene3d" in n), json.dumps(tree)


def edit(rpc, item, old, new):
    source = rpc.read_file_versioned(item, "main.lua")
    return rpc.edit_file(item, "main.lua", source["revision"], [{"old_text": old, "new_text": new}])


def sign_halves(rpc, item):
    """Screen colour a quarter of the way in from each side of the textured sign."""
    rect = next(r for r in rpc.rects() if r["id"] == "view")
    view = scene(rpc, item)[0]
    at = next(o for o in view["objects"] if o["id"] == "panel")["position"]
    shot = Image(base64.b64decode(rpc.screenshot(item)["png_base64"]))
    k = shot.width / 900
    out = []
    for dx in (-0.5, 0.5):
        x, y = project(view["camera"], [at[0] + dx, at[1], at[2]], rect)
        out.append(shot.at(x * k, y * k)[:3])
    return out


def close(a, b):
    return all(abs(x - y) < 1e-4 for x, y in zip(a, b))


with Session(shell_binary=shell_binary(), offscreen=(900, 700)) as s:
    rpc = s.rpc
    rpc.signup("glb", "glb smoke passphrase")
    ws = rpc.create_workspace("GLB")
    item = rpc.create_item(ws["id"], "GLB scene", "app")["id"]
    uploaded = rpc.upload_folder(item, APP)
    assert {"models/hierarchy.glb", "models/textured.glb"} <= set(uploaded), uploaded
    rpc.open_item(item)

    view, tree = scene(rpc, item)
    objects = {o["id"]: o for o in view["objects"]}
    assert set(objects) == {"base", "arm", "panel"}, objects
    assert close(objects["base"]["position"], [0, 0.5, 0]), objects["base"]
    # arm: parent translation + child translation, quarter turn about Y, own scale.
    assert close(objects["arm"]["position"], [0, 1.5, 0]), objects["arm"]
    s2 = math.sqrt(0.5)
    assert close(objects["arm"]["rotation"], [0, s2, 0, s2]), objects["arm"]
    assert close(objects["arm"]["scale"], [0.5, 2, 0.5]), objects["arm"]
    # Normals were missing and one triangle was zero-area: still a 12-triangle cube.
    assert objects["arm"]["triangles"] == 12, objects["arm"]
    assert "base <-> collider=box mass=120 tags=static+metal" in tree, tree
    assert "arm <base>" in tree and "marker <->" in tree, tree
    assert_3d_pixels(rpc, item, "view", "base")
    # The sign's material is white; only its texture makes the left half red, the right blue.
    assert objects["panel"]["texture"]["width"] == 8 and objects["panel"]["texture"]["height"] == 4
    assert objects["base"]["texture"] is None, objects["base"]
    left, right = sign_halves(rpc, item)
    assert left[0] > 2 * left[2] + 40 and right[2] > 2 * right[0] + 40, (left, right)
    assert not rpc.read_console(item), rpc.read_console(item)

    # Refusals: each names its reason, and the previous source keeps running.
    rpc.write_binary_file(item, "models/short.glb", (FIXTURES / "hierarchy.glb").read_bytes()[:200])
    rpc.write_binary_file(item, "models/external.glb", (FIXTURES / "external_buffer.glb").read_bytes())
    rpc.write_binary_file(item, "models/text.glb", b"not a model")
    rpc.write_binary_file(item, "models/big.glb", (FIXTURES / "oversize_image.glb").read_bytes())
    for path, why in [("models/missing.glb", "no binary file"), ("models/short.glb", "glb"),
                      ("models/external.glb", "external file"), ("models/text.glb", "not a binary glTF"),
                      ("models/big.glb", "4097x1")]:
        result = edit(rpc, item, '"models/hierarchy.glb"', f'"{path}"')
        assert result["persisted"] and why in str(result["activation"]), (path, result)
        assert {o["id"] for o in scene(rpc, item)[0]["objects"]} == {"base", "arm", "panel"}, path
        assert edit(rpc, item, f'"{path}"', '"models/hierarchy.glb"')["activation"] == "activated"

    # A fallback VM must also be able to load an asset it had not loaded through this handler.
    lazy_model = (FIXTURES / "hierarchy.glb").read_bytes().replace(b'"base"', b'"lazy"')
    assert lazy_model != (FIXTURES / "hierarchy.glb").read_bytes()
    rpc.write_binary_file(item, "models/lazy.glb", lazy_model)
    assert edit(rpc, item, "return ui.col({", '''return ui.col({
        ui.button({ id = "lazy-load", ui.text({ "Load again" }), on_click = function()
            local original = gfx.load_glb("models/lazy.glb")
            model.nodes[1].name = "lazy asset " .. original.nodes[1].name .. " by " .. me().name
        end }),''')["activation"] == "activated"

    # Replacing the model's bytes reloads the open app from the new file.
    rpc.write_binary_file(item, "models/hierarchy.glb", b"glTF broken")
    rpc.frame(1)
    assert {o["id"] for o in scene(rpc, item)[0]["objects"]} == {"base", "arm", "panel"}, "failed reload kept the old VM"
    assert any("glb" in line for line in rpc.read_console(item)), rpc.read_console(item)
    rpc.write_binary_file(item, "models/lazy.glb", b"glTF also broken")
    rpc.frame(1)
    rpc.click(item, "lazy-load")
    assert "lazy asset lazy by glb" in scene(rpc, item)[1], rpc.read_console(item)
    rpc.write_binary_file(item, "models/hierarchy.glb", (FIXTURES / "hierarchy.glb").read_bytes())
    rpc.write_binary_file(item, "models/lazy.glb", lazy_model)
    rpc.frame(1)
    assert scene(rpc, item)[0]["objects"], "repaired model did not load"
    rpc.save_screenshot(item, ROOT / "shots" / "glb-scene.png")
    print("glb scene ok: hierarchy, extras, generated normals, degenerate drop, texture pixels, refusals, live model reload")
