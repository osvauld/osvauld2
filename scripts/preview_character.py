#!/usr/bin/env python3
"""A converted character at a clip time, composited by ImageMagick beside the pack's own frame
(docs/design/2d-characters.md, step 4). Engine-free: a preview of the converter's output, not
proof of the engine; S-C4 in smoke_characters.py measures the shell's pixels the same way.

    python3 scripts/preview_character.py OUT_DIR [anim:frame ...]     # e.g. run:0 run:18

Each pair writes OUT_DIR/<anim>_f<frame>.png (pack | ours | overlap: red pack only, blue ours
only, grey both) and the table prints silhouette IoU and mean colour distance per frame."""

import math
import struct
import subprocess
import sys
import zlib
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from osvauld.png import Image  # noqa: E402
import test_spine_to_lua as T  # noqa: E402

APP = HERE.parent / "demo_apps" / "character_lab"
CHAR = APP / "characters" / "paka"
FRAMES = HERE / "fixtures" / "paka"
STAGE = (0x00, 0xFF, 0xFF)  # theme.lua's stage colour
# The lab's slot: the drawing box sits PAD in, inside SLOT (theme.lua).
PAD, SLOT = (20, 100), (480, 440)
# Non-background is farther from the stage than half the nearest art pixel (124): alpha ~ 1/2.
EDGE = 62


def png(w, h, rgb):
    raw = b"".join(b"\0" + bytes(c for x in range(w) for c in rgb(x, y)) for y in range(h))
    chunk = lambda kind, data: (struct.pack(">I", len(data)) + kind + data  # noqa: E731
                                + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF))
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b""))


def pixel(img, x, y):
    row = img.rows[y]
    return row[4 * x:4 * x + 4]


# --- ours: the rig posed by the engine's formulas (the converter test's evaluator) ----------

def pictures(rig, clip, t):
    """(image, src, matrix drawing-space) of each shown picture at t, in draw order."""
    poses = T.sample(clip, t)
    worlds = T.worlds(rig, poses)
    out = []
    for part in rig["parts"]:
        if not part.get("shapes") or not poses.get(part["id"], {}).get("show", True):
            continue
        for sh in part["shapes"]:
            out.append((sh["image"], sh["src"], T.mul(worlds[part["id"]], sh["transform"])))
    return out


def render(pics, size=SLOT, shift=PAD):
    """The pictures on the stage colour, drawing (x, y) at (x + shift), as the lab's slot shows
    them; an osvauld.png.Image."""
    cmd = ["magick", "-size", f"{size[0]}x{size[1]}", "xc:#00ffff"]
    for image, (x, y, w, h), m in pics:
        a, b, c, d, e, f = m
        cmd += ["(", str(APP / image), "-crop", f"{w}x{h}+{x}+{y}", "+repage", "-alpha", "on",
                "-virtual-pixel", "transparent", "-define", f"distort:viewport={size[0]}x{size[1]}+0+0",
                "-distort", "AffineProjection", f"{a},{b},{c},{d},{e + shift[0]},{f + shift[1]}",
                ")", "-composite"]
    cmd += ["-alpha", "on", "PNG32:-"]
    return Image(subprocess.run(cmd, capture_output=True, check=True).stdout)


# --- the comparison, shared with S-C4 -------------------------------------------------------

def pack_mask(img):
    return [sum(1 << x for x in range(img.width) if pixel(img, x, y)[3] > 127) for y in range(img.height)]


def ours_mask(img, box):
    """Rows of `img` inside box (x, y, w, h) as bit masks of non-background pixels."""
    x0, y0, w, h = box
    out = []
    for y in range(y0, y0 + h):
        bits = 0
        for x in range(w):
            r, g, b, _ = pixel(img, x0 + x, y)
            if max(abs(r - STAGE[0]), abs(g - STAGE[1]), abs(b - STAGE[2])) > EDGE:
                bits |= 1 << x
        out.append(bits)
    return out


def window(mask, box):
    x0, y0, w, h = box
    return [(mask[y] >> x0) & ((1 << w) - 1) for y in range(y0, y0 + h)]


def bbox(mask):
    ys = [y for y, row in enumerate(mask) if row]
    if not ys:
        return None
    lo = min((row & -row).bit_length() - 1 for row in mask if row)
    hi = max(row.bit_length() - 1 for row in mask if row)
    return lo, ys[0], hi, ys[-1]


def iou(a, b):
    inter = sum(bin(x & y).count("1") for x, y in zip(a, b))
    union = sum(bin(x | y).count("1") for x, y in zip(a, b))
    return inter / union if union else 0.0


def fit(pack, big, origin):
    """The pack canvas's place in drawing space: canvas (u, v) is drawing (u - dx, v - dy). First
    the two silhouettes' box centres, then the best IoU within 4 px. `big` is our image, drawing
    (0, 0) at `origin`; only the slot around it is the stage."""
    w, h = pack.width, pack.height
    pm = pack_mask(pack)
    x0, y0 = origin[0] - PAD[0], origin[1] - PAD[1]
    whole = [0] * y0 + [row << x0 for row in ours_mask(big, (x0, y0, SLOT[0], SLOT[1]))]
    pb, ob = bbox(pm), bbox(whole)
    gx = round((pb[0] + pb[2]) / 2 - ((ob[0] + ob[2]) / 2 - origin[0]))
    gy = round((pb[1] + pb[3]) / 2 - ((ob[1] + ob[3]) / 2 - origin[1]))
    best = None
    for dy in range(gy - 4, gy + 5):
        for dx in range(gx - 4, gx + 5):
            box = (origin[0] - dx, origin[1] - dy, w, h)
            if box[0] < 0 or box[1] < 0 or box[0] + w > big.width or box[1] + h > big.height:
                continue
            score = iou(pm, window(whole, box))
            if best is None or score > best[0]:
                best = (score, dx, dy)
    return best[1], best[2]


def score(pack, big, origin, d):
    """Silhouette IoU and the mean RGB distance (0-441) over the overlap, the pack's pixel taken
    over the stage colour as ours is."""
    w, h = pack.width, pack.height
    box = (origin[0] - d[0], origin[1] - d[1], w, h)
    pm, om = pack_mask(pack), ours_mask(big, box)
    total, n = 0.0, 0
    for y in range(h):
        both = pm[y] & om[y]
        while both:
            x = (both & -both).bit_length() - 1
            both &= both - 1
            r, g, b, a = pixel(pack, x, y)
            k = a / 255
            want = [c * k + s * (1 - k) for c, s in zip((r, g, b), STAGE)]
            got = pixel(big, box[0] + x, box[1] + y)[:3]
            total += math.dist(want, got)
            n += 1
    return iou(pm, om), (total / n if n else float("nan")), box


def side_by_side(pack, big, box, path):
    """pack over the stage | ours | the two silhouettes, each at 2x."""
    w, h = pack.width, pack.height
    pm, om = pack_mask(pack), ours_mask(big, box)

    def left(x, y):
        r, g, b, a = pixel(pack, x, y)
        k = a / 255
        return tuple(round(c * k + s * (1 - k)) for c, s in zip((r, g, b), STAGE))

    def mid(x, y):
        return tuple(pixel(big, box[0] + x, box[1] + y)[:3])

    def right(x, y):
        p, o = pm[y] >> x & 1, om[y] >> x & 1
        return (150, 150, 150) if p and o else (220, 40, 40) if p else (40, 80, 230) if o else (20, 20, 20)

    tmp = Path(path).with_suffix(".parts")
    tmp.mkdir(exist_ok=True)
    for name, f in (("a", left), ("b", mid), ("c", right)):
        (tmp / f"{name}.png").write_bytes(png(w, h, f))
    subprocess.run(["magick", str(tmp / "a.png"), str(tmp / "b.png"), str(tmp / "c.png"), "-bordercolor", "#000000",
                    "-border", "2", "+append", "-filter", "point", "-resize", "200%", str(path)], check=True)
    for f in tmp.iterdir():
        f.unlink()
    tmp.rmdir()


def frame_of(anim, frame):
    return next(f for f in (FRAMES / "frames").glob(f"{anim}_f{frame:03d}_t*.png"))


def main(argv):
    out = Path(argv[0])
    out.mkdir(parents=True, exist_ok=True)
    pairs = [a.split(":") for a in argv[1:]] or [["run", "0"], ["run", "18"], ["attack", "0"], ["attack", "20"],
                                                   ["jump", "0"], ["jump", "20"], ["idle", "0"], ["idle", "60"]]
    rig = T.lua_load(CHAR / "rig.lua")
    offsets, rows = {}, []
    for anim, frame in pairs:
        frame = int(frame)
        clip = T.lua_load(CHAR / "clips" / f"{anim}.lua")
        pack = Image(frame_of(anim, frame).read_bytes())
        big = render(pictures(rig, clip, frame / 60))
        if anim not in offsets:  # fitted on the first frame asked of each animation, then held
            offsets[anim] = fit(pack, big, PAD)
        s_iou, s_col, box = score(pack, big, PAD, offsets[anim])
        path = out / f"{anim}_f{frame:03d}.png"
        side_by_side(pack, big, box, path)
        rows.append((anim, frame, offsets[anim], s_iou, s_col, path))
    print(f"{'anim':7} {'frame':>5} {'offset':>10} {'IoU':>6} {'colour':>7}  image")
    for anim, frame, d, a, c, path in rows:
        print(f"{anim:7} {frame:5d} {str(d):>10} {a:6.3f} {c:7.1f}  {path}")


if __name__ == "__main__":
    main(sys.argv[1:])
