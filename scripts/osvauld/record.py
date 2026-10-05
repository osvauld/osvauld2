"""Scripted demo video of a live app, frame-exact on the offscreen virtual clock.

    with Recorder(session.rpc, item_id, Path("demo.mp4")) as rec:
        rec.glide_to("save", 0.6)      # an element id (aimed at its centre) or an (x, y) point
        rec.click()
        rec.zoom(1.8, 0.8)             # the app's own zoomable, Ctrl+wheel around the pointer
        rec.hold(1.0)                  # let the app's own animation play
    # rec.frames, and demo.mp4 (plus demo.gif with gif=True)

The pointer you see is the app's: mount `demo_apps/pointer/cursor.lua`. This file only moves the
real one. Design: `docs/design/demo-recorder.md`.

**The frame budget.** Every request that reaches the shell costs clock: a screenshot paints and
moves 1/60s, a pointer op 1/60s + 8ms. Frame k must be captured at exactly `t0 + (k+1)/fps`, so what
a frame does before its screenshot has to fit in `1/fps - 1/60` — one pointer op at 24fps. Each
verb issues at most one pointer op per frame, and a frame that overruns raises rather than
quietly shifting the video's time.
"""

import math
import shutil
import subprocess
from pathlib import Path
from typing import Callable

import base64

Point = tuple[float, float]


def ease(t: float) -> float:
    """Cubic in-out: a hand accelerates off the mark and settles onto the target."""
    return 4 * t**3 if t < 0.5 else 1 - (-2 * t + 2) ** 3 / 2


class Recorder:
    CLICK_HOLD = 0.125  # seconds the button stays down: long enough to see the press
    ZOOM_LINE = 1.1  # runtime/src/zoom.rs — one 30pt wheel line multiplies the scale by this

    def __init__(self, rpc, item_id: str, out: Path, *, fps: int = 24,
                 start: Point | None = None, gif: bool = False, crf: int = 18):
        if not shutil.which("ffmpeg"):
            raise RuntimeError("the recorder needs ffmpeg on PATH")
        self.rpc, self.item, self.out, self.fps = rpc, item_id, Path(out), fps
        self.start, self.gif, self.crf = start, gif, crf
        self.frames = 0
        self.pos: Point | None = None
        self.ffmpeg: subprocess.Popen | None = None

    # ── lifecycle ────────────────────────────────────────────────────────────
    def __enter__(self) -> "Recorder":
        self.out.parent.mkdir(parents=True, exist_ok=True)
        self.ffmpeg = subprocess.Popen(
            ["ffmpeg", "-y", "-v", "error", "-f", "image2pipe", "-framerate", str(self.fps),
             "-c:v", "png", "-i", "-",
             # yuv420p wants even dimensions; the shell's viewport need not be.
             "-vf", "pad=ceil(iw/2)*2:ceil(ih/2)*2",
             "-c:v", "libx264", "-preset", "medium", "-crf", str(self.crf),
             "-pix_fmt", "yuv420p", "-movflags", "+faststart", str(self.out)],
            stdin=subprocess.PIPE,
        )
        if self.start is not None:
            self.rpc.move_to(*self.start)
            self.pos = self.start
        self.t0 = self.clock()
        return self

    def __exit__(self, exc_type, *_):
        assert self.ffmpeg and self.ffmpeg.stdin
        self.ffmpeg.stdin.close()
        if self.ffmpeg.wait() != 0 and exc_type is None:
            raise RuntimeError(f"ffmpeg failed writing {self.out}")
        if self.gif and exc_type is None:
            gif = self.out.with_suffix(".gif")
            subprocess.run(
                ["ffmpeg", "-y", "-v", "error", "-i", str(self.out), "-vf",
                 "split[a][b];[a]palettegen=stats_mode=diff[p];[b][p]paletteuse=dither=sierra2_4a",
                 str(gif)],
                check=True,
            )

    # ── verbs ────────────────────────────────────────────────────────────────
    def glide_to(self, target: str | Point, secs: float = 0.6) -> None:
        """Move the pointer to an element's centre (or a point) along an eased path."""
        to = self.aim(target)
        frm = self.pos or to
        n = self.count(secs)
        for i in range(1, n + 1):
            f = ease(i / n)
            self.step(lambda p=(frm[0] + (to[0] - frm[0]) * f, frm[1] + (to[1] - frm[1]) * f): self.move(p))

    def click(self, hold: float = CLICK_HOLD) -> None:
        """Press where the pointer is, keep it down for `hold`, release."""
        n = max(2, self.count(hold))
        self.step(self.rpc.press)
        for _ in range(n - 2):
            self.step()
        self.step(self.rpc.release)

    def drag_to(self, target: str | Point, secs: float = 0.8) -> None:
        """Press, glide, release: the real gesture, so the app's drag and drop both fire."""
        self.step(self.rpc.press)
        self.glide_to(target, secs)
        self.step(self.rpc.release)

    def zoom(self, factor: float, secs: float = 0.8) -> None:
        """Multiply the zoom of the `zoomable` under the pointer by `factor`, eased, around the
        pointer — glide there first. The runtime clamps the scale to 0.4–3.0."""
        if self.pos is None:
            raise RuntimeError("zoom happens around the pointer: glide_to somewhere first")
        lines = math.log(factor) / math.log(self.ZOOM_LINE)
        n = self.count(secs)
        for i in range(1, n + 1):
            dy = 30 * lines * (ease(i / n) - ease((i - 1) / n))
            self.step(lambda dy=dy: self.rpc.wheel(*self.pos, 0, dy, ctrl=True))

    def hold(self, secs: float) -> None:
        """Script nothing; the app's own springs, fades and timers are what gets recorded."""
        for _ in range(self.count(secs)):
            self.step()

    # ── machinery ────────────────────────────────────────────────────────────
    def aim(self, target: str | Point) -> Point:
        if not isinstance(target, str):
            return target
        for r in self.rpc.rects():
            if r["id"] == target:
                return r["x"] + r["w"] / 2, r["y"] + r["h"] / 2
        raise RuntimeError(f"{target!r} is not reachable; it may exist but be clipped")

    def move(self, p: Point) -> None:
        self.rpc.move_to(*p)
        self.pos = p

    def count(self, secs: float) -> int:
        return max(1, round(secs * self.fps))

    def clock(self) -> float:
        return self.rpc.frame(0)["clock"]

    def step(self, op: Callable[[], object] | None = None) -> None:
        """One video frame: this frame's op, the clock brought to exactly its instant, a shot.
        Frame k is due at `t0 + (k+1)/fps`, so even the first one has a whole frame for its op."""
        if op is not None:
            op()
        due = self.t0 + (self.frames + 1) / self.fps
        now = self.clock()
        if now > due + 1e-9:
            raise RuntimeError(
                f"frame {self.frames} overran its budget by {now - due:.4f}s at {self.fps}fps; "
                "lower fps or split the step"
            )
        if now < due:
            self.rpc.advance(due - now)
        shot = self.rpc.screenshot(self.item)
        assert self.ffmpeg and self.ffmpeg.stdin
        self.ffmpeg.stdin.write(base64.b64decode(shot["png_base64"]))
        self.frames += 1
