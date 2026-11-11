#!/usr/bin/env python3
"""Contact sheets of clips on the robot (docs/design/brawl-movement.md §0, "checked by eye"). Drives
character_lab offscreen through its clip picker and exact time field, screenshots the Scout's slot at
n times across the clip and joins them left to right with ImageMagick, as preview_character.py does.

    OSVAULD_OFFSCREEN=1200x800 python3 scripts/clip_sheet.py [--n 8] [--out DIR] [clip ...]

With no clip names, every robot clip (cast.lua's `own`). Writes DIR/<clip>.png (default
shots/brawl/clips/). A looped clip is shot at i/n of its length, a once clip at i/(n-1), so both ends
show. A grey line under each frame is the feet line (the drawing's root pivot); a red one beside it
is the wall line, body_w / 2 in front of the centre.

For Brawl's moves (docs/design/brawl-fighting.md, the clip-authoring loop): `--ticks K` shoots move ticks
1, 1 + K, ... and the last, each at the frame that tick shows (act_t = tick - 1, quantized to 1/60 s) and
labelled by tick; `--hitboxes` draws the circles scripts/bake_brawl_moves.lua places on that tick, in red
(the lab draws the drawing unplanted, so they are in the drawing's own units, as the bake's `--dump`)."""

import argparse
import re
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from osvauld.session import Session, shell_binary  # noqa: E402

ROOT = HERE.parent
LAB = ROOT / "demo_apps" / "character_lab"
VIEWPORT = (1200, 800)
OWN = ["rise", "apex", "fall", "land", "jump2", "dash", "wall", "turn", "nlight", "slight", "dlight", "nheavy", "sheavy",
       "dheavy", "nair", "sair", "dair", "shair", "dhair", "dodge_spot", "dodge_dir", "jet", "jet_hover", "jet_thrust", "jet_cut"]
FEET, CENTRE, BODY_W = 239, 198, 80  # paka/rig.lua's root pivot; brawl's tune.lua body_w


def find(tree, prefix):
    if isinstance(tree, dict):
        for c in tree.get("children", []):
            if isinstance(c, str) and c.startswith(prefix):
                return c
        if isinstance(tree.get("text"), str) and tree["text"].startswith(prefix):
            return tree["text"]
        tree = tree.get("children", [])
    for child in tree if isinstance(tree, list) else []:
        got = find(child, prefix)
        if got:
            return got
    return None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("clips", nargs="*")
    ap.add_argument("--n", type=int, default=8)
    ap.add_argument("--out", default=str(ROOT / "shots" / "brawl" / "clips"))
    ap.add_argument("--ticks", type=int, default=0, help="a frame every K move ticks, labelled by tick")
    ap.add_argument("--hitboxes", action="store_true", help="draw the baked circles (moves only)")
    args = ap.parse_args()
    clips = args.clips or OWN
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    tmp = Path(tempfile.mkdtemp(prefix="clip-sheet-"))
    with Session(shell_binary=shell_binary(), offscreen=VIEWPORT) as s:
        r = s.rpc
        r.signup("sheets", "clip sheet passphrase")
        ws = r.create_workspace("sheets")
        item = r.create_item(ws["id"], "character lab", "app")["id"]
        r.upload_folder(item, LAB)
        r.open_item(item)
        r.frame(2)
        assert not r.read_console(item), r.read_console(item)
        for clip in clips:
            r.click(item, f"clip:{clip}")
            r.frame(1)
            line = find(r.dump_tree(item), "time: ")
            assert line, "no time readout"
            length = float(re.search(r"/ ([0-9.]+) s", line).group(1))
            src = LAB / "characters" / "robot" / "clips" / f"{clip}.lua"
            if not src.exists():
                src = LAB / "characters" / "paka" / "clips" / f"{clip}.lua"
            looped = "loop = true" in src.read_text()
            n = args.n
            times = [length * i / (n if looped else n - 1) for i in range(n)]
            labels = [f"{clip} {t:.3f}s" for t in times]
            circles = {}
            if args.ticks:
                total = round(length * 120)
                ks = list(range(1, total + 1, args.ticks))
                if ks[-1] != total:
                    ks.append(total)
                times = [((k - 1) // 2) / 60 for k in ks]
                labels = [f"{clip} t{k}" for k in ks]
                if args.hitboxes:
                    dump = subprocess.run(["lua", "scripts/bake_brawl_moves.lua", "--dump", clip], cwd=ROOT, capture_output=True,
                                          text=True, check=True).stdout
                    for line in dump.splitlines():
                        k = int(line.split()[1])
                        cs = line.split("circles", 1)[1].strip()
                        circles[k] = [tuple(map(float, c.split())) for c in cs.split("|") if c.strip()]
                    circles = [circles.get(k, []) for k in ks]
            frames = []
            for i, t in enumerate(times):
                r.type_text(item, "time", f"{t:.6f}")
                r.frame(1)
                console = r.read_console(item)
                assert not console, console
                rect = {x["id"]: x for x in r.rects()}["char:scout"]
                shot = tmp / f"{clip}_{i}.png"
                r.save_screenshot(item, str(shot))
                # the slot: the drawing box is at rect's top-left (the frame keeps its box)
                x0, y0 = int(rect["x"]) + 60, int(rect["y"]) - 70
                w, h = (440 if args.ticks else 280), 380  # a move's blade reaches past the narrow slot
                fx, fy = FEET, CENTRE
                cropped = tmp / f"{clip}_{i}_c.png"
                subprocess.run(["magick", str(shot), "-crop", f"{w}x{h}+{x0}+{y0}", "+repage",
                                "-stroke", "#1d2231", "-draw", f"line 0,{int(rect['y']) + FEET - y0} {w},{int(rect['y']) + FEET - y0}",
                                "-stroke", "#ff3030", "-draw",
                                f"line {int(rect['x']) + CENTRE + BODY_W // 2 - x0},0 {int(rect['x']) + CENTRE + BODY_W // 2 - x0},{h}",
                                *[a for (cx, cy, cr) in (circles[i] if circles else []) for a in (
                                    "-fill", "#ff304055", "-stroke", "#ff3040", "-draw",
                                    f"circle {int(rect['x']) + cx - x0:.1f},{int(rect['y']) + cy - y0:.1f} {int(rect['x']) + cx + cr - x0:.1f},{int(rect['y']) + cy - y0:.1f}")],
                                "-fill", "#1d2231", "-stroke", "none", "-font", "Adwaita-Sans", "-pointsize", "14", "-annotate", "+6+18", labels[i],
                                str(cropped)], check=True)
                frames.append(str(cropped))
            dest = out / f"{clip}.png"
            subprocess.run(["magick", *frames, "+append", str(dest)], check=True)
            print(f"{clip}: {length:.4f} s {'loop' if looped else 'once'}, {n} frames at "
                  f"{', '.join(f'{t:.3f}' for t in times)} -> {dest}")


if __name__ == "__main__":
    main()
