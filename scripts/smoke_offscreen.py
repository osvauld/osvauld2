"""End-to-end smoke for offscreen mode: drive a real app with no window.

    python3 scripts/smoke_offscreen.py

What it pins is the pair of claims offscreen mode is built on, because each one can pass
vacuously on its own:

- **Real pixels.** A screenshot that is uniformly the background colour would satisfy any
  size assertion, so this counts distinct colours and looks for the app's own palette.
- **A virtual clock, exact and driven.** Each delivered request paints exactly one frame and
  moves the clock 1/60s, so 120 requests is 2.000s — not "about two seconds". The pomodoro
  is the instrument: its face is derived from a deadline, so a clock that does not advance
  reads 25:00 forever and a clock that free-runs on real time drifts off 24:58.

Needs a display server even though it shows nothing — winit will not build an event loop
without one. `xvfb-run -a python3 scripts/smoke_offscreen.py` covers a machine with no
display; removing the dependency means replacing the event loop (`design/six-apps.md` §7).
"""

import base64
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from osvauld.session import Session, build_shell, shell_binary

VIEWPORT = (900, 700)
BG = "#12131a"  # demo_apps/pomodoro/theme.lua — C.bg


def labels(tree) -> list[str]:
    """Every string the tree draws, in paint order."""
    found: list[str] = []

    def walk(node) -> None:
        if isinstance(node, dict):
            for key in ("text", "label"):
                if isinstance(node.get(key), str):
                    found.append(node[key])
            for value in node.values():
                walk(value)
        elif isinstance(node, list):
            for value in node:
                walk(value)

    walk(tree)
    return found


def palette(png: bytes) -> dict[str, int]:
    """Sampled colour histogram, so "did it draw anything" is answerable without Pillow."""
    import struct
    import zlib

    pos, idat, width, height = 8, b"", 0, 0
    while pos < len(png):
        (size,) = struct.unpack(">I", png[pos : pos + 4])
        kind = png[pos + 4 : pos + 8]
        if kind == b"IHDR":
            width, height = struct.unpack(">II", png[pos + 8 : pos + 16])
        elif kind == b"IDAT":
            idat += png[pos + 8 : pos + 8 + size]
        pos += 12 + size

    raw = zlib.decompress(idat)
    stride, cursor, previous, counts = width * 4, 0, bytearray(width * 4), {}
    for _ in range(height):
        filter_type, cursor = raw[cursor], cursor + 1
        line = bytearray(raw[cursor : cursor + stride])
        cursor += stride
        for i in range(stride):
            left = line[i - 4] if i >= 4 else 0
            up = previous[i]
            up_left = previous[i - 4] if i >= 4 else 0
            if filter_type == 1:
                line[i] = (line[i] + left) & 255
            elif filter_type == 2:
                line[i] = (line[i] + up) & 255
            elif filter_type == 3:
                line[i] = (line[i] + (left + up) // 2) & 255
            elif filter_type == 4:
                guess = left + up - up_left
                dl, du, dul = abs(guess - left), abs(guess - up), abs(guess - up_left)
                nearest = left if (dl <= du and dl <= dul) else (up if du <= dul else up_left)
                line[i] = (line[i] + nearest) & 255
        for x in range(0, width, 7):
            key = "#%02x%02x%02x" % tuple(line[x * 4 : x * 4 + 3])
            counts[key] = counts.get(key, 0) + 1
        previous = line
    return counts


build_shell()
with Session(shell_binary=shell_binary(), offscreen=VIEWPORT) as s:
    assert s.rpc.ping() == "pong"
    s.rpc.signup("abe", "correct horse")
    ws = s.rpc.create_workspace("demo")
    item = s.rpc.create_item(ws["id"], "pomodoro", "app")["id"]
    s.rpc.upload_folder(item, Path(__file__).parent.parent / "demo_apps" / "pomodoro")
    assert s.rpc.open_item(item) == "open"

    # ── real pixels, with no window and no surface ──────────────────────────────
    shot = s.rpc.screenshot(item)
    counts = palette(base64.b64decode(shot["png_base64"]))
    assert len(counts) > 20, f"a window-less frame should be a real app, got {counts}"
    assert counts.get(BG, 0) > 100, f"the pomodoro background is missing: {sorted(counts)[:8]}"

    # ── the clock is virtual, driven, and exact ─────────────────────────────────
    assert "Start" in labels(s.rpc.dump_tree(item))
    s.rpc.click(item, "toggle")
    s.rpc.frame()  # one frame; the frame is what reads the clock
    assert "Pause" in labels(s.rpc.dump_tree(item)), "on_frame never ran offscreen"

    # 120 frames at 1/60s is 2.000s, and the op reports the clock so the assertion is on time
    # itself rather than on counting requests and trusting the arithmetic.
    before = s.rpc.frame(0)["clock"]
    moved = s.rpc.frame(120)
    assert moved["frames"] == 120
    assert abs(moved["clock"] - before - 2.0) < 1e-9, moved
    face = [t for t in labels(s.rpc.dump_tree(item)) if ":" in t]
    assert face == ["24:58"], f"expected exactly 2.000s of virtual time, face reads {face}"

    # Advance is the one that makes a 25-minute timer testable: jump past the whole session and
    # the app finishes it, which no number of frames could reach (a real session is 90,000).
    s.rpc.advance(25 * 60)
    assert labels(s.rpc.dump_tree(item))[0] == "Break", "the work session never finished"
    assert s.rpc.read_data(item)["pomodoro"]["stats"]["done"] == 1.0

    # ── rects: where a pointer must land ───────────────────────────────────────
    # The entry gap-log 1.4 was written about. App 1 cost fifteen cold-start runs sweeping
    # coordinates for this button; it is one request now. Asserted as geometry rather than
    # "is present", because a rect that exists and is wrong is the failure that actually hurt.
    at = {r["id"]: r for r in s.rpc.rects()}
    toggle, reset = at["toggle"], at["reset"]
    assert toggle["hits"] == ["click"], toggle
    assert toggle["w"] > 0 and toggle["h"] > 0, toggle

    # The two buttons sit in a row with `gap = 10`, and the first wrong guess in app 1 landed
    # exactly in that gap — so the gap is the thing worth pinning, not the absolute positions.
    gap = reset["x"] - (toggle["x"] + toggle["w"])
    assert abs(gap - 10.0) < 0.5, f"buttons are {gap}pt apart, theme says 10"
    assert abs(toggle["y"] - reset["y"]) < 0.5, "same row"

    # ── the real pointer pipeline, aimed by rects ──────────────────────────────
    # This is the capability `open.rs` uniquely had: a hit-test, not a handler called by id.
    # The loop is rects -> aim -> click -> observe, which is the one app 1 could not close.
    assert labels(s.rpc.dump_tree(item))[2] == "Start", "the break timer should be idle"
    under = s.rpc.click_at(*s.rpc.centre_of("toggle"))
    assert [r["id"] for r in under] == ["toggle"], f"aimed at toggle, was over {under}"
    s.rpc.frame()
    assert labels(s.rpc.dump_tree(item))[2] == "Pause", "a coordinate click did not start it"

    # And the negative, which is the half that actually hurt: a miss must report as a miss rather
    # than as silence. 1.4 was expensive because "nothing happened" and "you were 200pt out" look
    # identical from the outside.
    gap_x = toggle["x"] + toggle["w"] + 5  # dead centre of the 10pt gap between the buttons
    assert s.rpc.move_to(gap_x, toggle["y"] + toggle["h"] / 2) == []

    assert s.rpc.read_console(item) == []

print("offscreen smoke ok")
