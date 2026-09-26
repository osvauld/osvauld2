#!/usr/bin/env python3
"""Upload the Lua Frame-orbits demo into a fresh shell and capture its live render."""

import argparse
import json
import time
from pathlib import Path

from osvauld.session import Session, build_shell

ROOT = Path(__file__).resolve().parent.parent
APP = ROOT / "demo_apps" / "frame_orbits"


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=ROOT / "shots" / "frame-orbits.png")
    parser.add_argument("--size", nargs=2, type=float, default=(1000, 700), metavar=("WIDTH", "HEIGHT"))
    parser.add_argument("--scale", type=float, default=1.0)
    parser.add_argument("--delay", type=float, default=0.75, help="seconds between motion proofs")
    parser.add_argument("--tree", type=Path, help="optional DumpTree JSON destination")
    parser.add_argument("--verbose", action="store_true")
    args = parser.parse_args()

    output = args.output.expanduser().resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    build_shell()
    with Session(show_shell_output=args.verbose) as session:
        session.rpc.signup("frame-screenshot", "test")
        workspace = session.rpc.create_workspace("Frame demos")
        item = session.rpc.create_item(workspace["id"], "Frame orbits", "app")
        files = session.rpc.upload_folder(item["id"], APP)
        session.rpc.open_item(item["id"])
        session.rpc.reload_item(item["id"])

        console = session.rpc.read_console(item["id"])
        if console:
            raise RuntimeError("Lua console is not clean:\n" + "\n".join(console))
        tree = session.rpc.dump_tree(item["id"])
        if args.tree:
            args.tree.write_text(json.dumps(tree, indent=2), encoding="utf-8")
        size = {"width": args.size[0], "height": args.size[1], "scale": args.scale}
        before = output.with_name(f"{output.stem}-before{output.suffix}")
        session.rpc.save_screenshot(item["id"], before, **size)
        time.sleep(args.delay)
        dimensions = session.rpc.save_screenshot(item["id"], output, **size)
        print(f"uploaded {len(files)} files: {', '.join(files)}")
        print(f"motion proof: {before} -> {output} ({args.delay:.2f}s)")
        print(f"final size: {dimensions['width_px']}x{dimensions['height_px']} px")


if __name__ == "__main__":
    main()
