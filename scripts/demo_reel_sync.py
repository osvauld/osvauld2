#!/usr/bin/env python3
"""Record the "two users, one node" clip: alice and bob side by side, one chat, one kunki node.

    python3 scripts/demo_reel_sync.py [--out shots/reel/sync_two_users] [--fps 24]

Two offscreen shells, stepped in lockstep: every video frame advances both virtual clocks by
1/fps and takes a screenshot of each. A push travels on the wall clock, so after a message is
sent the script waits (real time, on the other peer's data) until it arrived, then plays a few
frames so it is on screen. Needs release `shell2` and `kunki` (cargo build --release) and a DISPLAY.
"""

import argparse
import base64
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from osvauld import net as netmod  # noqa: E402
from osvauld.net import Net, wait_until  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
CHAT = ROOT / "demo_apps" / "chat"
VIEWPORT = "700x640"
CHROME_TOP = 38  # the shell's tab bar


class Pair:
    """Both peers' screens, one frame at a time."""

    def __init__(self, alice, bob, item: str, fps: int, tmp: Path):
        self.peers = (alice, bob)
        self.item, self.fps = item, fps
        self.dirs = [tmp / "alice", tmp / "bob"]
        for d in self.dirs:
            d.mkdir()
        self.frames = 0

    def frame(self) -> None:
        for peer, d in zip(self.peers, self.dirs):
            peer.rpc.advance(1 / self.fps)
            shot = peer.rpc.screenshot(self.item)
            (d / f"{self.frames:05d}.png").write_bytes(base64.b64decode(shot["png_base64"]))
        self.frames += 1

    def hold(self, secs: float) -> None:
        for _ in range(max(1, round(secs * self.fps))):
            self.frame()

    def type_out(self, peer, text: str, per_frame: int = 2) -> None:
        """The composer fills a couple of characters a frame, like typing."""
        for n in range(per_frame, len(text) + per_frame, per_frame):
            peer.rpc.type_text(self.item, "composer", text[:n])
            self.frame()

    def send(self, peer, other, text: str) -> None:
        self.type_out(peer, text)
        self.hold(0.3)
        peer.rpc.key(self.item, "composer", "enter")
        self.frame()
        wait_until(lambda: text in str(other.rpc.read_data(self.item)), 8.0,
                   f"{other.name} to see {text!r}")
        self.hold(1.4)


def encode(tmp: Path, out: Path, fps: int) -> None:
    crop = f"crop=iw:ih-{CHROME_TOP}:0:{CHROME_TOP}"
    inputs = ["-framerate", str(fps), "-i", str(tmp / "alice" / "%05d.png"),
              "-framerate", str(fps), "-i", str(tmp / "bob" / "%05d.png")]
    stack = f"[0]{crop}[a];[1]{crop}[b];[a][b]hstack=inputs=2"
    out.parent.mkdir(parents=True, exist_ok=True)
    mp4 = out.with_suffix(".mp4")
    subprocess.run(["ffmpeg", "-y", "-v", "error", *inputs, "-filter_complex",
                    stack + ",pad=ceil(iw/2)*2:ceil(ih/2)*2", "-c:v", "libx264", "-crf", "18",
                    "-pix_fmt", "yuv420p", "-movflags", "+faststart", str(mp4)], check=True)
    gif = out.with_suffix(".gif")
    subprocess.run(["ffmpeg", "-y", "-v", "error", "-i", str(mp4), "-vf",
                    "fps=12,scale=iw*0.7:-1:flags=lanczos,split[a][b];"
                    "[a]palettegen=stats_mode=diff[p];[b][p]paletteuse=dither=sierra2_4a",
                    str(gif)], check=True)
    print(f"→ {mp4} ({mp4.stat().st_size // 1024} KB), {gif} ({gif.stat().st_size // 1024} KB)")


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--out", type=Path, default=ROOT / "shots" / "reel" / "sync_two_users")
    ap.add_argument("--fps", type=int, default=24)
    args = ap.parse_args()

    # net.py defaults to debug binaries; the reel records release ones.
    netmod.KUNKI_BINARY = ROOT / "target" / "release" / "kunki"
    netmod.SHELL_BINARY = ROOT / "target" / "release" / "shell2"
    import os
    os.environ.setdefault("OSVAULD_OFFSCREEN", VIEWPORT)

    tmp = Path(tempfile.mkdtemp(prefix="osvauld-reel-sync-"))
    try:
        with Net(shots=str(tmp / "shots")) as net:
            alice, bob = net.peer("alice"), net.peer("bob")
            ws, item = net.share_app(alice, [bob], CHAT)
            item_id = item["id"]
            for p in (alice, bob):
                p.rpc.frame(3)
            pair = Pair(alice, bob, item_id, args.fps, tmp)
            pair.hold(0.8)
            pair.send(alice, bob, "Morning! Standup in 5?")
            pair.send(bob, alice, "On my way. Same room, same data.")
            pair.send(alice, bob, "Nothing left our own node.")
            pair.hold(0.8)
            print(f"{pair.frames} frames")
        encode(tmp, args.out, args.fps)
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


if __name__ == "__main__":
    main()
