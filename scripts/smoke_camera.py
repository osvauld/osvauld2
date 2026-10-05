"""C10 (docs/design/camera.md): a camera through the real shell. The hero stays in the middle of
the box while the map moves under it; the frame holds only what the box can show; the pointer
hits what is drawn where it lands. Pixels, rects and the dump — never "nothing errored"."""

import base64
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from osvauld.png import Image, near
from osvauld.session import Session, shell_binary

VIEWPORT = (1000, 700)
HERO, STONE, GROUND = "#e0453a", "#3a6fe0", "#2a3b33"


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


def world_of(tree, wid):
    if tree.get("id") == wid:
        return tree["world"]
    for child in tree.get("children", []):
        found = world_of(child, wid)
        if found:
            return found
    return None


with Session(shell_binary=shell_binary(), offscreen=VIEWPORT) as s:
    s.rpc.signup("camera", "camera passphrase")
    ws = s.rpc.create_workspace("camera smoke")
    item = s.rpc.create_item(ws["id"], "camera", "app")["id"]
    s.rpc.upload_folder(item, Path(__file__).parent.parent / "demo_apps" / "camera")
    s.rpc.open_item(item)
    s.rpc.frame(2)
    assert not s.rpc.read_console(item), s.rpc.read_console(item)
    box = next(r for r in s.rpc.rects() if r["id"] == "map")
    assert (box["w"], box["h"]) == (640, 400), box

    def camera():
        return world_of(s.rpc.dump_tree(item), "map")["camera"]

    def pixel(x, y):
        """The colour at a point of the box, in the box's own units."""
        shot = s.rpc.screenshot(item)
        img = Image(base64.b64decode(shot["png_base64"]))
        k = img.width / VIEWPORT[0]
        return img.at((box["x"] + x) * k, (box["y"] + y) * k)

    def entity_at(x, y):
        s.rpc.move_to(box["x"] + x, box["y"] + y)
        return readout(s.rpc.dump_tree(item), "entity: ")

    def stone_on_screen(i, j, at):
        """Where stone_i_j's centre shows, for a camera at `at` (the world point mid-box)."""
        return (100 * i + 50 - at[0] + 320, 100 * j + 50 - at[1] + 200)

    # The hero's box centre is (1500, 1000): the camera starts there, not easing in from 0, 0.
    cam = camera()
    assert cam["at"] == [1500, 1000] and cam["follow"] == "hero" and not cam["lost"], cam
    assert cam["bounds"] == [0, 0, 3000, 2000] and cam["view"] == [640, 400], cam
    # View x 1180..1820, y 800..1200; a stone's box grown 12 each side meets it for columns
    # 12..17 and rows 8..11: 24 stones and the hero, of 601.
    assert cam["drawn"] == 25, cam

    assert near(pixel(320, 200), HERO), pixel(320, 200)
    sx, sy = stone_on_screen(14, 9, cam["at"])
    assert (sx, sy) == (270, 150)
    assert near(pixel(sx, sy), STONE), pixel(sx, sy)
    assert entity_at(sx, sy) == "stone_14_9"
    assert entity_at(300, 150) == "—", "between stones: nothing"

    # Walk right through the real keyboard path. The camera follows exactly (no ease): the hero
    # stays mid-box and the stones shift left by however far it went.
    s.rpc.keyboard("KeyD", "d", True)
    s.rpc.frame(60)
    s.rpc.keyboard("KeyD", "d", False)
    s.rpc.frame(2)
    assert not s.rpc.read_console(item), s.rpc.read_console(item)
    hero = next(e for e in world_of(s.rpc.dump_tree(item), "map")["entities"] if e["id"] == "hero")
    cam = camera()
    assert 1700 < cam["at"][0] < 1800 and cam["at"][1] == 1000, cam
    assert abs(cam["at"][0] - (hero["pos"][0] + 16)) < 1e-6, (cam, hero)
    assert near(pixel(320, 200), HERO), "the hero is still mid-box"
    sx, sy = stone_on_screen(14, 9, cam["at"])
    assert 0 < sx < 100, sx
    assert near(pixel(sx, sy), STONE), pixel(sx, sy)
    assert entity_at(sx, sy) == "stone_14_9", "the pointer hits the stone where it is drawn now"
    assert 0 < cam["drawn"] < 40, cam

    # Up against the map's right edge, the camera stops: the hero walks off-centre.
    s.rpc.keyboard("KeyD", "d", True)
    s.rpc.frame(60 * 6)
    s.rpc.keyboard("KeyD", "d", False)
    s.rpc.frame(2)
    cam = camera()
    assert cam["at"][0] == 3000 - 320, cam
    assert not near(pixel(320, 200), HERO), "the camera stopped, the hero did not"

    png = base64.b64decode(s.rpc.screenshot(item)["png_base64"])
    if len(sys.argv) > 1:
        Path(sys.argv[1]).write_bytes(png)
    print("camera: follows, culls to 25 of 601, hits what is drawn, stops at bounds")
