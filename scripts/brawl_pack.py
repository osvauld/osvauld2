"""Rebuild demo_apps/brawl/art/ from the Scavenge Legends pack zip (CC0, Dumivid), per art/SOURCES.txt.

    python3 -I scripts/brawl_pack.py "<path to>/Scavenge Legends Complete World V1.1.zip" [--check] [--grid]

Each SOURCES.txt line is `out ← pack path [crop WxH+X+Y] [alpha a]`. A plain line copies the pack's
bytes; a crop or alpha goes through ImageMagick. A file whose pixels already match is left alone, so
a rebuild is no churn. Prints each file's size and opaque box (alpha > 0, and alpha >= 0.5), the box
the anchors in maps/assets/*.lua are measured from. --check writes nothing and fails on a mismatch.
--grid writes shots/brawl/tilemap-grid.png: _tilemap_main.png cut into 128 px cells, each labelled
col,row and its opaque coverage. The zip stays outside the repo; it is read, never extracted whole.
"""

import argparse
import re
import shutil
import subprocess
import sys
import tempfile
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ART = ROOT / "demo_apps" / "brawl" / "art"
SOURCES = ART / "SOURCES.txt"
TILEMAP = "128 pixels (recommended size for full HD)/_tilemap_main.png"
LINE = re.compile(r"^(?P<out>[a-z0-9_]+\.png) ← (?P<src>.+?)(?: crop (?P<crop>\d+x\d+\+\d+\+\d+))?(?: alpha (?P<alpha>[0-9.]+))?$")


def magick(*args, out=False):
    r = subprocess.run(["magick", *map(str, args)], check=True, capture_output=True)
    return r.stdout if out else None


def alpha_of(path):
    w, h = map(int, magick(path, "-format", "%w %h", "info:", out=True).split())
    a = magick(path, "-alpha", "on", "-alpha", "extract", "-depth", "8", "gray:-", out=True)
    assert len(a) == w * h, path
    return w, h, a


def box(w, h, a, at):
    xs, ys = [], []
    for y in range(h):
        row = a[y * w:(y + 1) * w]
        hit = [x for x, v in enumerate(row) if v >= at]
        if hit:
            xs += (hit[0], hit[-1])
            ys.append(y)
    if not ys:
        return None
    return min(xs), ys[0], max(xs) - min(xs) + 1, ys[-1] - ys[0] + 1


def describe(path):
    w, h, a = alpha_of(path)
    any_, half = box(w, h, a, 1), box(w, h, a, 128)
    fmt = lambda b: "none" if b is None else "%dx%d+%d+%d" % (b[2], b[3], b[0], b[1])
    return w, h, "size %dx%d  opaque>0 %s  opaque>=0.5 %s" % (w, h, fmt(any_), fmt(half))


def same_pixels(a, b):
    sig = lambda p: magick(p, "-alpha", "on", "-format", "%w %h %#", "info:", out=True)
    return sig(a) == sig(b)


def sources():
    out = []
    for n, line in enumerate(SOURCES.read_text(encoding="utf-8").splitlines(), 1):
        if not line.strip() or line.startswith("#"):
            continue
        m = LINE.match(line)
        if not m:
            sys.exit(f"SOURCES.txt:{n}: not `out ← pack path [crop WxH+X+Y] [alpha a]`: {line}")
        out.append(m.groupdict())
    return out


def build(zf, entry, tmp):
    raw = tmp / ("raw_" + entry["out"])
    raw.write_bytes(zf.read(entry["src"]))
    if not entry["crop"] and not entry["alpha"]:
        return raw
    made = tmp / entry["out"]
    args = [raw, "-alpha", "on"]
    if entry["crop"]:
        args += ["-crop", entry["crop"], "+repage"]
    if entry["alpha"]:
        args += ["-channel", "A", "-evaluate", "multiply", entry["alpha"], "+channel"]
    magick(*args, "PNG32:" + str(made))
    return made


def grid(zf, tmp):
    sheet = ROOT / "shots" / "brawl" / "tilemap-grid.png"
    sheet.parent.mkdir(parents=True, exist_ok=True)
    src = tmp / "tilemap.png"
    src.write_bytes(zf.read(TILEMAP))
    w, h, a = alpha_of(src)
    cells = []
    for row in range(h // 128):
        for col in range(w // 128):
            cov = sum(1 for y in range(row * 128, row * 128 + 128) for v in a[y * w + col * 128:y * w + col * 128 + 128] if v > 0)
            cell = tmp / f"cell_{row:02d}_{col:02d}.png"
            magick(src, "-alpha", "on", "-crop", f"128x128+{col * 128}+{row * 128}", "+repage",
                   "(", "-size", "128x128", "pattern:checkerboard", "-fill", "#c8c8c8", "-opaque", "black", ")",
                   "+swap", "-compose", "over", "-composite", cell)
            cells.append((cell, f"{col},{row} x{col * 128} y{row * 128} {cov * 100 // (128 * 128)}%"))
    args = ["montage"]
    for cell, label in cells:
        args += ["-label", label, cell]
    args += ["-tile", f"{w // 128}x", "-geometry", "128x128+6+6", "-font", "Adwaita-Sans", "-pointsize", "13", "-background", "#2b2b33",
             "-fill", "#ffffff", sheet]
    subprocess.run(args, check=True, capture_output=True)
    print(f"tilemap grid: {sheet.relative_to(ROOT)} ({w // 128}x{h // 128} cells of 128 px)")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("zip")
    ap.add_argument("--check", action="store_true", help="write nothing; fail if art/ differs")
    ap.add_argument("--grid", action="store_true", help="also write shots/brawl/tilemap-grid.png")
    args = ap.parse_args()
    zf = zipfile.ZipFile(args.zip)
    entries = sources()
    named = {e["out"] for e in entries}
    on_disk = {p.name for p in ART.glob("*.png")}
    bad = 0
    with tempfile.TemporaryDirectory(prefix="brawl-pack-") as t:
        tmp = Path(t)
        for e in entries:
            made = build(zf, e, tmp)
            dest = ART / e["out"]
            if dest.exists() and same_pixels(made, dest):
                state = "same"
            elif args.check:
                state, bad = "DIFFERS", bad + 1
            else:
                shutil.copyfile(made, dest)
                state = "wrote"
            print(f"{e['out']:<24} {state:<7} {describe(dest if dest.exists() else made)[2]}")
        lic = zf.read("License.txt")
        if (ART / "License.txt").read_bytes() != lic:
            if args.check:
                bad += 1
                print("License.txt DIFFERS")
            else:
                (ART / "License.txt").write_bytes(lic)
                print("License.txt wrote")
        for extra in sorted(on_disk - named):
            print(f"{extra}: in art/ but not in SOURCES.txt")
            bad += 1
        if args.grid:
            grid(zf, tmp)
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
