"""Focused release-compatible interaction check; no writes to an existing user's store."""
import math
import os
from osvauld.session import Session, shell_binary


def node(tree, el_id):
    if tree.get("id") == el_id:
        return tree
    for child in tree.get("children", []):
        found = node(child, el_id)
        if found:
            return found
    return None


def texts(tree):
    return [tree.get("text", "")] + [text for child in tree.get("children", []) for text in texts(child)]


def camera(rpc, item):
    return node(rpc.dump_tree(item), "scene-viewport")["scene3d"]["camera"]["eye"]


with Session(shell_binary=shell_binary(), offscreen=(1000, 800),
             show_shell_output=os.environ.get("OSVAULD_FRAME_TIMINGS") == "1") as s:
    rpc = s.rpc
    rpc.signup("interaction check", "passphrase")
    ws = rpc.create_workspace("Interaction check")
    viewer = rpc.create_item(ws["id"], "Current model viewer", "app")["id"]
    rpc.upload_folder(viewer, "demo_apps/model_viewer")
    rpc.open_item(viewer)
    viewport = node(rpc.dump_tree(viewer), "scene-viewport")
    assert set(viewport["handlers"]) >= {"on_click", "on_drag", "on_wheel"}
    centre = rpc.centre_of("scene-viewport")
    rpc.click_at(*centre)
    assert any(text in ("blue-cube", "coral-cube") for text in texts(rpc.dump_tree(viewer)))
    before = camera(rpc, viewer)
    rect = next(r for r in rpc.rects() if r["id"] == "scene-viewport")
    end = (centre[0] + min(80, rect["w"] / 4), centre[1] + min(40, rect["h"] / 4))
    rpc.drag(centre, end, steps=8)
    after = camera(rpc, viewer)
    assert sum(abs(a - b) for a, b in zip(before, after)) > .1, "orbit did not move the camera"
    rpc.wheel(*centre, 0, 30)
    zoomed = camera(rpc, viewer)
    assert math.dist(zoomed, (0, 0, 0)) < math.dist(after, (0, 0, 0)), "wheel did not zoom"
    rpc.save_screenshot(viewer, "/tmp/model-viewer-interaction.png")
    assert rpc.read_console(viewer) == []
    print("viewer: pointer picking, orbit drag and wheel zoom passed")

    kanban = rpc.create_item(ws["id"], "Current kanban", "app")["id"]
    rpc.upload_folder(kanban, "demo_apps/kanban")
    rpc.open_item(kanban)
    start = rpc.centre_of("k1")
    target = rpc.centre_of("k3")
    rpc.drag(start, target, steps=12)
    board = rpc.read_data(kanban)["board"]
    assert next(card for card in board["cards"] if card["id"] == "k1")["col"] == "c-doing"
    rpc.save_screenshot(kanban, "/tmp/kanban-interaction.png")
    assert rpc.read_console(kanban) == []
    print("kanban: cross-column card drag committed to the document")

print("interaction acceptance passed; screenshots in /tmp/model-viewer-interaction.png and /tmp/kanban-interaction.png")
