"""One drawing module, four poses: gfx.drawing compiles, poses render, parts answer hits by id."""

import base64
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from osvauld.session import Session, shell_binary


def readout(tree, prefix):
    if isinstance(tree, dict):
        if str(tree.get("text", "")).startswith(prefix):
            return tree["text"][len(prefix):]
        tree = list(tree.values())
    if isinstance(tree, list):
        for child in tree:
            found = readout(child, prefix)
            if found is not None:
                return found
    return None


with Session(shell_binary=shell_binary(), offscreen=(900, 700)) as s:
    s.rpc.signup("hero", "hero passphrase")
    ws = s.rpc.create_workspace("hero smoke")
    item = s.rpc.create_item(ws["id"], "hero", "app")["id"]
    s.rpc.upload_folder(item, Path(__file__).parent.parent / "demo_apps" / "hero")
    s.rpc.open_item(item)
    s.rpc.frame(2)
    assert not s.rpc.read_console(item), s.rpc.read_console(item)

    rects = {r["id"]: r for r in s.rpc.rects()}
    for pose in ("rest", "wave", "step", "lean"):
        assert f"pose:{pose}" in rects, sorted(rects)

    # A posed part answers the hit test by its drawing id: (80, 70) is the head, (80, 130) the body.
    rest = rects["pose:rest"]
    for (x, y), part in (((80, 70), "head"), ((80, 130), "body")):
        s.rpc.move_to(rest["x"] + x, rest["y"] + y)
        label = readout(s.rpc.dump_tree(item), "part: ")
        assert label == f"rest / {part}", label

    png = base64.b64decode(s.rpc.screenshot(item)["png_base64"])
    if len(sys.argv) > 1:
        Path(sys.argv[1]).write_bytes(png)
    print("hero: 4 poses reachable, parts hit by id")
