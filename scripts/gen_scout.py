"""Generates the stock scout (pilot format v2, drawn at 2x) into both baked pilot files.

    python3 scripts/gen_scout.py

Our own art: a helmeted soldier with a jetpack, drawn facing right with both arms reaching
straight forward (the rig bends them, scripts/test_jetpack_rig.lua). Rewrites
demo_apps/jetpack/baked/pilots.lua and the pilot editor's copy; re-bake from an editor to keep edits.
"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from bake_jetpack import source

ROOT = Path(__file__).resolve().parent.parent
n = 0


def sid():
    global n
    n += 1
    return f"s{n}"


def r2(v):
    return round(v, 2)


def poly(pts, fill):
    return {"id": sid(), "fill": "@" + fill,
            "path": [["move", *pts[0]]] + [["line", *p] for p in pts[1:]] + [["close"]]}


def rect(x, y, w, h, fill):
    return poly([[x, y], [x + w, y], [x + w, y + h], [x, y + h]], fill)


def rrect(x, y, w, h, r, fill):
    return {"id": sid(), "fill": "@" + fill, "path": [
        ["move", x + r, y], ["line", x + w - r, y], ["quad", x + w, y, x + w, y + r],
        ["line", x + w, y + h - r], ["quad", x + w, y + h, x + w - r, y + h],
        ["line", x + r, y + h], ["quad", x, y + h, x, y + h - r],
        ["line", x, y + r], ["quad", x, y, x + r, y], ["close"]]}


def ellipse(x, y, w, h, fill):
    rx, ry = w / 2, h / 2
    cx, cy = x + rx, y + ry
    k = 0.5523
    return {"id": sid(), "fill": "@" + fill, "path": [
        ["move", r2(cx + rx), r2(cy)],
        ["cubic", r2(cx + rx), r2(cy + ry * k), r2(cx + rx * k), r2(cy + ry), r2(cx), r2(cy + ry)],
        ["cubic", r2(cx - rx * k), r2(cy + ry), r2(cx - rx), r2(cy + ry * k), r2(cx - rx), r2(cy)],
        ["cubic", r2(cx - rx), r2(cy - ry * k), r2(cx - rx * k), r2(cy - ry), r2(cx), r2(cy - ry)],
        ["cubic", r2(cx + rx * k), r2(cy - ry), r2(cx + rx), r2(cy - ry * k), r2(cx + rx), r2(cy)],
        ["close"]]}


# Rest pose: arm bones straight along the row of the shoulder.
# gun arm (front): shoulder, elbow, wrist; free arm (back): the same, a little higher and forward.
# Arms are long (2b, 2026-10-27): the free arm reaches the support of a full-size (0.5) gun, the gun arm
# holds the grip with a ~90 degree elbow instead of a deep fold. Lengths: upper arm + forearm.
GUN_L = (UP_G, FORE_G) = (21, 19)
FREE_L = (UP_F, FORE_F) = (27, 25)
GUN = {"s": (27, 58), "e": (27 + UP_G, 58), "w": (27 + UP_G + FORE_G, 58)}
FREE = {"s": (28, 56), "e": (28 + UP_F, 56), "w": (28 + UP_F + FORE_F, 56)}
HOLD = (HOLD_X, HOLD_Y) = (58, 60)  # where the gun's grip sits, from the pilot's origin
SHIFT = 8  # the whole drawing moves right so the wider canvas keeps the body where the 80-wide one had it
WIDTH = 104


def sleeve(c, colour, pad):
    """An upper arm from shoulder to elbow: ink outline, sleeve, optionally a shoulder pad."""
    (sx, sy), (ex, _) = c["s"], c["e"]
    out = [rrect(sx - 6, sy - 7, ex - sx + 12, 14, 7, "ink"), rrect(sx - 4, sy - 5, ex - sx + 8, 10, 5, colour)]
    if pad:
        out += [ellipse(sx - 7, sy - 9, 17, 17, "ink"), ellipse(sx - 5, sy - 7, 13, 13, "main"),
                rect(sx - 3, sy - 1, 9, 2, "dark")]
    return out


def forearm(c, colour):
    (ex, ey), (wx, _) = c["e"], c["w"]
    return [rrect(ex - 6, ey - 7, wx - ex + 10, 14, 7, "ink"), rrect(ex - 4, ey - 5, wx - ex + 6, 10, 5, colour),
            rect(wx - 5, ey - 6, 4, 12, "steel"), rect(wx - 5, ey - 6, 1.5, 12, "ink")]


def fist(c, dy=0):
    """The glove at the wrist, fingers curled, pointing forward."""
    wx, wy = c["w"]
    return [ellipse(wx - 3, wy - 8.5, 16, 17, "ink"), ellipse(wx - 1, wy - 6.5, 12, 13, "glove"),
            ellipse(wx + 4, wy - 3, 9, 5, "ink"), ellipse(wx + 4.5, wy - 2.5, 8, 4, "glove"),
            ellipse(wx + 4, wy + 2, 9, 5, "ink"), ellipse(wx + 4.5, wy + 2.5, 8, 4, "glove"),
            rect(wx + 1, wy - 6, 1.5, 12, "ink")]


def leg(x, colour):
    return [poly([[x, 77], [x + 11, 77], [x + 11, 97], [x, 97]], "ink"),
            poly([[x + 1.5, 77], [x + 9.5, 77], [x + 9.5, 96], [x + 1.5, 96]], colour),
            rect(x + 1.5, 86, 8, 4, "dark"),
            poly([[x - 1, 94], [x + 11, 94], [x + 12, 97], [x + 17, 99], [x + 17, 103], [x - 1, 103]], "ink"),
            rect(x - 1, 100.5, 18, 2.5, "steel"),
            rect(x + 12, 99, 4, 1.2, "glint")]


head = [
    rect(27, 1, 3, 7, "ink"), ellipse(25.5, -0.5, 6, 5, "ink"),
    ellipse(8, 6, 42, 42, "ink"), ellipse(11, 9, 36, 36, "dark"), ellipse(11, 9, 36, 31, "main"),
    ellipse(10, 21, 13, 15, "ink"), ellipse(12, 23, 9, 11, "dark"),
    rrect(27, 18, 24, 21, 8, "ink"), rrect(30, 21, 18, 15, 6, "visor"),
    poly([[33, 23], [40, 23], [36, 30], [33, 30]], "glint"), rect(41, 24, 3, 3, "glint"),
    rect(13, 15, 14, 2.5, "dark"),
]
torso = [
    rect(23, 43, 10, 8, "dark"),
    poly([[14, 46], [44, 46], [46, 81], [12, 81]], "ink"),
    poly([[16, 48], [42, 48], [43.5, 79], [14.5, 79]], "main"),
    poly([[16, 48], [24, 48], [41, 76], [33, 76]], "dark"),
    rrect(26, 60, 14, 12, 2, "ink"), rrect(27.5, 61.5, 11, 9, 1.5, "dark"), rect(27.5, 64, 11, 1.2, "ink"),
    rect(12, 73, 34, 8, "ink"), rect(13.5, 74.5, 31, 5, "dark"),
    rect(25, 74, 9, 6, "ink"), rect(26.5, 75.2, 6, 3.6, "steel"), rect(26.5, 75.2, 6, 1, "glint"),
]
jetpack = [
    rrect(0, 45, 20, 40, 5, "ink"), rrect(2, 47, 16, 36, 4, "steel"),
    rect(7, 49, 2, 32, "ink"), rect(11, 49, 2, 32, "ink"),
    rect(2, 58, 16, 4, "main"), rect(2, 58, 16, 1, "glint"), rect(2, 69, 16, 3, "dark"),
    rect(17, 52, 6, 24, "ink"), rect(18, 54, 4, 20, "dark"),
    poly([[3, 85], [17, 85], [15, 91], [5, 91]], "ink"), poly([[5, 85], [15, 85], [14, 89], [6, 89]], "steel"),
]

parts = [
    ("arm_free", None, FREE["s"], sleeve(FREE, "dark", False)),
    ("forearm_free", "arm_free", FREE["e"], forearm(FREE, "dark")),
    ("hand_free", "forearm_free", FREE["w"], fist(FREE)),
    ("leg_back", None, (26, 78), leg(21, "dark")),
    ("leg_front", None, (36, 78), leg(31, "steel")),
    ("jetpack", None, (11, 62), jetpack),
    ("torso", None, (28, 64), torso),
    ("head", "torso", (28, 46), head),
    ("arm_gun", None, GUN["s"], sleeve(GUN, "dark", True)),
    ("forearm_gun", "arm_gun", GUN["e"], forearm(GUN, "dark")),
    ("hand_gun", "forearm_gun", GUN["w"], fist(GUN)),
    ("finger_gun", "hand_gun", (GUN["w"][0] + 7, GUN["w"][1] - 2), []),
    ("thumb_gun", "hand_gun", (GUN["w"][0] + 2, GUN["w"][1] - 6), []),
]
# The trigger finger and thumb, over the fist.
fx, fy = GUN["w"][0] + 7, GUN["w"][1] - 2
parts[11] = (*parts[11][:3], [rrect(fx - 2, fy - 3, 9, 6, 3, "ink"), rrect(fx - 1, fy - 2, 7, 4, 2, "glove")])
tx, ty = GUN["w"][0] + 2, GUN["w"][1] - 6
parts[12] = (*parts[12][:3], [rrect(tx - 1, ty - 4, 12, 7, 3.5, "ink"), rrect(tx, ty - 3, 10, 5, 2.5, "glove")])

pilot = {
    "id": "scout", "name": "Scout", "size": [WIDTH, 104], "scale": 0.5,
    "vars": {"main": "#d5e3a8", "dark": "#709a78", "ink": "#182e34", "visor": "#7fdddc", "glint": "#deffff",
             "steel": "#344e50", "glove": "#e6bf80"},
    "points": {"palm_gun": [GUN["w"][0] + 6, GUN["w"][1] + 1], "palm_free": [FREE["w"][0] + 6, FREE["w"][1] + 1],
               "hold": list(HOLD), "jet": [10, 88], "eye": [40, 28]},
    "parts": [{"id": i, **({"parent": p} if p else {}), "pivot": list(pv), "shapes": sh} for i, p, pv, sh in parts],
}


def shift_path(path):
    out = []
    for c in path:
        if c[0] == "close":
            out.append(c)
        else:
            out.append([c[0]] + [r2(x + SHIFT) if i % 2 == 0 else x for i, x in enumerate(c[1:])])
    return out


for part in pilot["parts"]:
    part["pivot"][0] += SHIFT
    for sh in part["shapes"]:
        sh["path"] = shift_path(sh["path"])
for pt in pilot["points"].values():
    pt[0] += SHIFT
text = source("jetpack-pilots", {"info": {"version": 2}, "pilots": [pilot]})
for app in ("jetpack", "jetpack_pilot_editor", "jetpack_gun_editor"):
    (ROOT / "demo_apps" / app / "baked" / "pilots.lua").write_text(text)
print(f"scout: {sum(len(p['shapes']) for p in pilot['parts'])} shapes")
