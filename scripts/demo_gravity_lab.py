#!/usr/bin/env python3
"""Record the gravity lab ("Siege") as one continuous take: mp4, plus a GIF with --gif.

    python3 scripts/demo_gravity_lab.py [--out shots/reel] [--gif]

Offscreen and frame-exact, like `demo_pointer_video.py`: the same run is the same video. The shell's
tab bar is cropped off. `gravity_lab(rec, rpc, item)` is a clip in `demo_reel.py`'s shape, so the
reel can import it: `from demo_gravity_lab import gravity_lab`.
"""

import argparse
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from osvauld.record import Recorder
from osvauld.session import Session, build_shell, shell_binary

ROOT = Path(__file__).resolve().parent.parent
VIEWPORT = (1000, 700)
CHROME_TOP = 38  # the shell's tab bar


def gravity_lab(rec: Recorder, rpc, item: str) -> None:
    """Aim with the dotted arc and fire; the castle falls, the dominoes run, the marble rolls into
    the bucket; then a hand throws a crate and the moon takes the gravity away."""

    def at(id: str) -> tuple[float, float]:
        def find(t):
            if t.get("id") == "lab":
                return {e["id"]: e for e in t["world"]["entities"]}
            for c in t.get("children", []):
                if found := find(c):
                    return found

        return find(rpc.dump_tree(item))[id]["pos"]

    lab = next(r for r in rpc.rects() if r["id"] == "lab")
    ox, oy = lab["x"], lab["y"]
    rec.hold(1.2)
    rec.glide_to((ox + 96, oy + 392), 0.9)  # onto the stone
    rec.step(rpc.press)
    rec.glide_to((ox + 96 - 62, oy + 392 + 38), 1.0)  # pull back: the arc previews the shot
    rec.hold(0.9)
    rec.step(rpc.release)
    rec.glide_to((ox + 480, oy + 60), 1.4)  # out of the way while it plays
    rec.hold(5.0)  # castle, dominoes, marble, bucket
    rec.glide_to("swing", 0.7)
    rec.click()  # the wrecking ball swings through what is left
    rec.hold(1.6)
    x, y = at("seesaw_c1")
    rec.glide_to((ox + x + 11, oy + y + 12), 0.9)
    rec.step(rpc.press)
    rec.glide_to((ox + x + 11 + 140, oy + y + 12 - 70), 0.35)
    rec.step(rpc.release)  # let go while moving: a throw
    rec.hold(1.0)
    rec.glide_to("gravity", 0.7)
    rec.click()  # the moon
    rec.glide_to("bomb", 0.5)
    rec.click()  # and the rubble floats up
    rec.hold(3.6)


def crop(src: Path, dst: Path) -> None:
    subprocess.run(["ffmpeg", "-y", "-v", "error", "-i", str(src), "-vf", f"crop=iw:ih-{CHROME_TOP}:0:{CHROME_TOP}",
                    "-c:v", "libx264", "-preset", "medium", "-crf", "18", "-pix_fmt", "yuv420p",
                    "-movflags", "+faststart", str(dst)], check=True)


def small_gif(src: Path, dst: Path, fps: int = 13, width: int = 660) -> None:
    vf = f"fps={fps},scale={width}:-1:flags=lanczos,split[a][b];[a]palettegen=stats_mode=diff[p];[b][p]paletteuse=dither=sierra2_4a"
    subprocess.run(["ffmpeg", "-y", "-v", "error", "-i", str(src), "-vf", vf, str(dst)], check=True)


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--out", type=Path, default=ROOT / "shots" / "reel")
    ap.add_argument("--gif", action="store_true")
    args = ap.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    raw = args.out / "gravity_lab.raw.mp4"
    final = args.out / "gravity_lab.mp4"

    build_shell(release=True)
    with Session(shell_binary=shell_binary(release=True), offscreen=VIEWPORT) as s:
        rpc = s.rpc
        rpc.signup("reel", "correct horse")
        ws = rpc.create_workspace("reel")
        item = rpc.create_item(ws["id"], "Siege", "app")["id"]
        rpc.upload_folder(item, ROOT / "demo_apps" / "gravity_lab")
        rpc.open_item(item)
        rpc.frame(3)
        with Recorder(rpc, item, raw, start=(500, 600)) as rec:
            gravity_lab(rec, rpc, item)
        errors = rpc.read_console(item)
    crop(raw, final)
    raw.unlink()
    if args.gif:
        small_gif(final, final.with_suffix(".gif"))
    print(f"{rec.frames} frames → {final}" + (f"  console: {errors[:3]}" if errors else ""))


if __name__ == "__main__":
    main()
