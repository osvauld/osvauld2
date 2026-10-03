#!/usr/bin/env python3
"""Record the pointer showcase (`demo_apps/pointer`) as an mp4 (and a GIF with --gif).

    python3 scripts/demo_pointer_video.py [--out shots/pointer.mp4] [--gif]

Offscreen and frame-exact: the same script always records the same video. The cursor is the app's
Lua cursor; its look comes from what it is over; the zoom is the board's own.
"""

import argparse
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from osvauld.record import Recorder
from osvauld.session import Session, build_shell, shell_binary

ROOT = Path(__file__).resolve().parent.parent
APP = ROOT / "demo_apps" / "pointer"
VIEWPORT = (900, 700)


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--out", type=Path, default=ROOT / "shots" / "pointer.mp4")
    ap.add_argument("--gif", action="store_true", help="also write a GIF beside the mp4")
    args = ap.parse_args()

    build_shell()
    with Session(shell_binary=shell_binary(), offscreen=VIEWPORT) as s:
        rpc = s.rpc
        rpc.signup("demo", "correct horse")
        ws = rpc.create_workspace("demo")
        item = rpc.create_item(ws["id"], "Pointer", "app")["id"]
        rpc.upload_folder(item, APP)
        rpc.open_item(item)
        rpc.frame(2)

        with Recorder(rpc, item, args.out, start=(450, 640), gif=args.gif) as rec:
            rec.hold(0.4)
            rec.glide_to("one", 0.7)
            rec.click()
            rec.hold(0.3)
            rec.glide_to("two", 0.4)
            rec.click()
            rec.click()
            rec.hold(0.3)
            rec.glide_to("card-b", 0.8)  # the cursor turns into a hand over a card
            rec.click()
            rec.hold(0.4)
            rec.zoom(1.8, 1.0)
            rec.hold(0.6)
            rec.glide_to((760, 560), 0.6)  # off the cards: the board's own magnifier
            rec.hold(0.5)
            rec.zoom(1 / 1.8, 0.8)
            rec.hold(0.6)
        print(f"{rec.frames} frames → {args.out}" + (f" (+ {args.out.with_suffix('.gif')})" if args.gif else ""))


if __name__ == "__main__":
    main()
