"""End-to-end smoke for the demo recorder (`docs/design/demo-recorder.md` §0, S1–S4).

    python3 scripts/smoke.py smoke_demo_record.py

The pointer is drawn by Lua (`demo_apps/pointer/cursor.lua`), zoom is the app's own `zoomable`
driven by Ctrl+wheel, and a recording is screenshots on the virtual clock. Each claim is checked
in pixels or rects, never by "nothing errored".
"""

import base64
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from osvauld.png import Image, near
from osvauld.session import Session, build_shell, shell_binary

APP = Path(__file__).parent.parent / "demo_apps" / "pointer"
VIEWPORT = (900, 700)
UP, DOWN = "#f5f5f5", "#ffc933"  # cursor.lua — C.up, C.down
INSIDE = (3, 12)  # a point inside the arrow, from its tip


def shot(rpc, item) -> tuple[Image, float]:
    """The frame, and physical pixels per logical point."""
    s = rpc.screenshot(item)
    img = Image(base64.b64decode(s["png_base64"]))
    return img, img.width / VIEWPORT[0]


def cursor_pixel(rpc, item, at, d=INSIDE):
    img, k = shot(rpc, item)
    return img.at((at[0] + d[0]) * k, (at[1] + d[1]) * k)


def rect(rpc, el_id):
    for r in rpc.rects():
        if r["id"] == el_id:
            return r
    raise AssertionError(f"{el_id} is not reachable")


def text_of(tree, el_id):
    if isinstance(tree, dict):
        if tree.get("id") == el_id:
            return tree.get("text")
        tree = list(tree.values())
    if isinstance(tree, list):
        for v in tree:
            if (t := text_of(v, el_id)) is not None:
                return t
    return None


build_shell()
with Session(shell_binary=shell_binary(), offscreen=VIEWPORT) as s:
    rpc = s.rpc
    rpc.signup("demo", "correct horse")
    ws = rpc.create_workspace("demo")
    item = rpc.create_item(ws["id"], "pointer", "app")["id"]
    rpc.upload_folder(item, APP)
    assert rpc.open_item(item) == "open"
    rpc.frame(2)
    assert not rpc.read_console(item), rpc.read_console(item)

    # ── S1: the Lua cursor sits on the pointer, shows the button, takes no click ────
    one = rect(rpc, "one")
    at = (one["x"] + one["w"] / 2, one["y"] + one["h"] / 2)
    rpc.move_to(*at)
    rpc.frame(10)  # a new look fades in over 90ms
    assert near(cursor_pixel(rpc, item, at), UP), cursor_pixel(rpc, item, at)
    rpc.press()
    rpc.frame(1)
    assert near(cursor_pixel(rpc, item, at), DOWN), cursor_pixel(rpc, item, at)
    rpc.release()
    rpc.frame(30)
    assert near(cursor_pixel(rpc, item, at), UP), cursor_pixel(rpc, item, at)
    assert text_of(rpc.dump_tree(item), "one-count") == "1", "the click under the cursor was lost"
    print("S1 ok: cursor on the pointer, down shown, click passed through")

    # ── S2: Ctrl+wheel is the board's real zoom; the cursor is not zoomed ────────────
    card = rect(rpc, "card-a")
    corner = (card["x"] + 2, card["y"] + 2)
    rpc.wheel(*corner, 0, 60, ctrl=True)
    rpc.frame(10)
    zoomed = rect(rpc, "card-a")
    assert abs(zoomed["w"] - card["w"] * 1.21) < 0.5, (card, zoomed)
    # Over a card the cursor is the hand, hotspot where fingers meet palm: 5pt below is palm.
    assert near(cursor_pixel(rpc, item, corner, (0, 5)), UP), "cursor gone after zoom"
    below = cursor_pixel(rpc, item, corner, (0, 24))  # past the hand's 10pt below the hotspot
    assert not near(below, UP), "the cursor grew with the board"
    print("S2 ok: ctrl+wheel zoomed the board ×1.21, cursor stayed pointer-sized")

    # ── S5: the thing under the pointer chooses the look; the root's cursor draws it ──
    def ids(tree, out=None):
        out = set() if out is None else out
        if isinstance(tree, dict):
            if isinstance(tree.get("id"), str):
                out.add(tree["id"])
            tree = list(tree.values())
        if isinstance(tree, list):
            for v in tree:
                ids(v, out)
        return out

    def look_at(point):
        rpc.move_to(*point)
        rpc.frame(1)
        return {i for i in ids(rpc.dump_tree(item)) if i.startswith("cursor:")}

    one = rect(rpc, "one")
    card = rect(rpc, "card-b")
    last = rect(rpc, "card-c")  # the board has no handler, so no rect: aim at it past the cards
    assert look_at((card["x"] + 20, card["y"] + 20)) == {"cursor:grab"}
    assert look_at((last["x"] + last["w"] + 40, last["y"] + last["h"] + 80)) == {"cursor:visual"}
    assert look_at((one["x"] + 10, one["y"] + 10)) == {"cursor:arrow"}
    print("S5 ok: card → grab, board → its own drawing, button → arrow")


# ── S3/S4: a scripted recording is frame-exact, shows the cursor, and is deterministic ──
import hashlib
import subprocess
import tempfile

from osvauld.record import Recorder

FPS = 24


def script(rec: Recorder) -> None:
    rec.glide_to("one", 0.5)
    rec.click()
    rec.glide_to("card-b", 0.6)
    rec.zoom(1.6, 0.8)
    rec.hold(0.4)
    rec.zoom(1 / 1.6, 0.6)


# Each verb's frames, independently of the recorder: round(secs × fps), a click is its hold.
EXPECTED = sum(round(s * FPS) for s in (0.5, Recorder.CLICK_HOLD, 0.6, 0.8, 0.4, 0.6))
AFTER_GLIDE = round(0.5 * FPS) - 1  # the last frame of the first glide: cursor on "one"


def record(out: Path) -> tuple[dict, str]:
    """A fresh shell, the script recorded to `out`; returns the target rect and the final tree."""
    with Session(shell_binary=shell_binary(), offscreen=VIEWPORT) as s:
        rpc = s.rpc
        rpc.signup("demo", "correct horse")
        ws = rpc.create_workspace("demo")
        item = rpc.create_item(ws["id"], "pointer", "app")["id"]
        rpc.upload_folder(item, APP)
        rpc.open_item(item)
        rpc.frame(2)
        one = rect(rpc, "one")
        with Recorder(rpc, item, out, fps=FPS, start=(VIEWPORT[0] / 2, VIEWPORT[1] - 80)) as rec:
            script(rec)
        assert rec.frames == EXPECTED, (rec.frames, EXPECTED)
        return one, text_of(rpc.dump_tree(item), "one-count")


def ffprobe_frames(path: Path) -> int:
    out = subprocess.run(
        ["ffprobe", "-v", "error", "-count_frames", "-select_streams", "v:0",
         "-show_entries", "stream=nb_read_frames", "-of", "csv=p=0", str(path)],
        check=True, capture_output=True, text=True,
    ).stdout
    return int(out.strip())


def frame_png(path: Path, n: int) -> Image:
    png = subprocess.run(
        ["ffmpeg", "-v", "error", "-i", str(path), "-vf", f"select=eq(n\\,{n})",
         "-frames:v", "1", "-f", "image2pipe", "-c:v", "png", "-pix_fmt", "rgba", "-"],
        check=True, capture_output=True,
    ).stdout
    return Image(png)


def framemd5(path: Path) -> str:
    out = subprocess.run(
        ["ffmpeg", "-v", "error", "-i", str(path), "-f", "framemd5", "-"],
        check=True, capture_output=True, text=True,
    ).stdout
    lines = [l for l in out.splitlines() if not l.startswith("#")]
    return hashlib.sha256("\n".join(lines).encode()).hexdigest()


with tempfile.TemporaryDirectory() as tmp:
    first, second = Path(tmp) / "a.mp4", Path(tmp) / "b.mp4"
    one, count = record(first)
    assert ffprobe_frames(first) == EXPECTED, (ffprobe_frames(first), EXPECTED)
    img = frame_png(first, AFTER_GLIDE)
    k = img.width / VIEWPORT[0]
    tip = (one["x"] + one["w"] / 2, one["y"] + one["h"] / 2)
    px = img.at((tip[0] + INSIDE[0]) * k, (tip[1] + INSIDE[1]) * k)
    assert near(px, UP, 40), f"frame {AFTER_GLIDE} has no cursor at the target: {px}"
    assert count == "1", f"the recorded click did not land: {count!r}"
    print(f"S3 ok: {EXPECTED} frames at {FPS}fps, cursor on target in frame {AFTER_GLIDE}, click landed")

    record(second)
    assert framemd5(first) == framemd5(second), "two recordings of one script differ"
    print("S4 ok: the same script records the same frames")
