#!/usr/bin/env python3
"""Upload kanban into a fresh shell, open it, and save a live screenshot."""

import argparse
from pathlib import Path

from osvauld.session import Session

ROOT = Path(__file__).resolve().parent.parent
KANBAN = ROOT / "shell2" / "src" / "kanban"


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument(
        "--output",
        type=Path,
        default=ROOT / "kanban-screenshot.png",
        help="PNG destination (default: ./kanban-screenshot.png)",
    )
    ap.add_argument(
        "--size",
        nargs=2,
        type=float,
        metavar=("WIDTH", "HEIGHT"),
        help="custom logical viewport; omit for the exact visible window",
    )
    ap.add_argument("--scale", type=float, help="physical pixels per logical point")
    ap.add_argument("--no-wait", action="store_true", help="close the shell after capture")
    ap.add_argument("--verbose", action="store_true", help="show shell logs")
    args = ap.parse_args()

    output = args.output.expanduser().resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    session = Session(show_shell_output=args.verbose)
    session.start()
    try:
        session.rpc.signup("screenshot-test", "test")
        workspace = session.rpc.create_workspace("Screenshot test")
        item = session.rpc.create_item(workspace["id"], "Kanban", "app")
        files = session.rpc.upload_folder(item["id"], KANBAN)
        session.rpc.open_item(item["id"])

        width, height = args.size if args.size else (None, None)
        dimensions = session.rpc.save_screenshot(
            item["id"],
            output,
            width=width,
            height=height,
            scale=args.scale,
        )
        print(f"uploaded {len(files)} files and opened kanban")
        print(
            f"screenshot stored at: {output} "
            f"({dimensions['width_px']}x{dimensions['height_px']} px)"
        )
        if not args.no_wait:
            input("Press Enter to close the shell... ")
    finally:
        session.close()


if __name__ == "__main__":
    main()
