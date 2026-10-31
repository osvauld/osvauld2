"""Generate the six Jetpack Arena guns (design/jetpack-militia.md, "Gun format") into baked/guns.lua.

    python3 scripts/gen_guns.py

Drawn at 2x, pointing right, y down. Each part is outlined in ink (every element grown by OUT),
then filled, then highlighted, so overlapping elements merge into one silhouette.
"""
import math
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from bake_jetpack import source

ROOT = Path(__file__).resolve().parent.parent
OUT = 1.2

PALETTE = {"ink": "#182e34", "edge": "#729a87", "metal": "#4a636b", "dark": "#2c4349",
           "grip": "#6b4a38", "wood": "#9a6a45", "warm": "#e99563", "gold": "#fbd484"}


def rect(x0, y0, x1, y1, c=0):
    if not c:
        return [(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
    return [(x0 + c, y0), (x1 - c, y0), (x1, y0 + c), (x1, y1 - c),
            (x1 - c, y1), (x0 + c, y1), (x0, y1 - c), (x0, y0 + c)]


def grow(pts, d):
    """Offset a polygon outward by d (miter), whatever its winding."""
    n = len(pts)
    area = sum(pts[i][0] * pts[(i + 1) % n][1] - pts[(i + 1) % n][0] * pts[i][1] for i in range(n))
    sign = 1 if area > 0 else -1
    def normal(a, b):
        dx, dy = b[0] - a[0], b[1] - a[1]
        ln = math.hypot(dx, dy) or 1
        return (sign * dy / ln, -sign * dx / ln)
    out = []
    for i in range(n):
        n1 = normal(pts[i - 1], pts[i])
        n2 = normal(pts[i], pts[(i + 1) % n])
        k = 1 + n1[0] * n2[0] + n1[1] * n2[1]
        k = max(k, 0.5)
        out.append((pts[i][0] + (n1[0] + n2[0]) / k * d, pts[i][1] + (n1[1] + n2[1]) / k * d))
    return out


def arc(p0, p1, bulge, n=6):
    """Points along a quadratic curve from p0 to p1 with its control pushed by bulge (x, y)."""
    cx, cy = (p0[0] + p1[0]) / 2 + bulge[0], (p0[1] + p1[1]) / 2 + bulge[1]
    return [((1 - t) ** 2 * p0[0] + 2 * (1 - t) * t * cx + t * t * p1[0],
             (1 - t) ** 2 * p0[1] + 2 * (1 - t) * t * cy + t * t * p1[1])
            for t in (i / n for i in range(1, n))]


# Game units per drawing unit, per gun: the drafted 0.5, the scout's own scale (step 2b, 2026-10-27: the scout's
# arms grew so the free hand reaches a support 24 drawing units from the grip at full size; the old 0.34/0.4 are gone).
SCALES = {"pistol": 0.5, "smg": 0.5, "rifle": 0.5, "shotgun": 0.5, "sniper": 0.5, "rocket": 0.5}


class Gun:
    def __init__(self, id, name, size, vars, points, stats):
        self.id, self.name, self.size, self.vars, self.points, self.stats = id, name, size, vars, points, stats
        self.parts, self.n = [], 0

    def part(self, id, pivot, elems):
        """elems: (pts, fill) filled and outlined; (pts, fill, False) filled only (highlights, details)."""
        shapes = []
        def add(fill, pts):
            self.n += 1
            r = lambda v: round(v * 2) / 2
            path = [["move", r(pts[0][0]), r(pts[0][1])]] + [["line", r(x), r(y)] for x, y in pts[1:]] + [["close"]]
            shapes.append({"id": f"s{self.n}", "fill": fill, "path": path})
        for e in elems:
            if len(e) == 2:
                add("@ink", grow(e[0], OUT))
        for e in elems:
            add("@" + e[1] if not e[1].startswith("#") else e[1], e[0])
        self.parts.append({"id": id, "pivot": list(pivot), "shapes": shapes})

    def data(self):
        return {"id": self.id, "name": self.name, "size": list(self.size), "scale": SCALES[self.id],
                "vars": self.vars, "parts": self.parts, "points": self.points, "stats": self.stats}


def vars_(accent):
    v = dict(PALETTE)
    v["accent"] = accent
    return v


def stats(**kw):
    base = dict(kind="bullet", pellets=1, auto=False, zooms=[1], melee=25)
    base.update(kw)
    return base


def pistol():
    g = Gun("pistol", "Pistol", (46, 32), vars_("#e99563"),
            {"grip": [14, 17], "muzzle": [43, 9], "eject": [27, 6], "mag": [13, 23]},
            stats(damage=22, fire_interval=0.28, auto=False, shot_speed=900, spread=0.02, mag_size=12,
                  reload_time=1.1, range=360, recoil=4, zooms=[1, 1.25], melee=20))
    g.part("mag", (13, 23), [(rect(9.5, 22, 17.5, 30, 1), "dark"), (rect(9, 28, 18, 30.5), "accent"),
                             (rect(10.8, 23, 12, 28), "metal", False)])
    g.part("body", (14, 17), [
        (rect(6, 13, 31, 19, 1), "metal"),
        ([(11, 13), (23, 13), (20, 24), (8, 24)], "grip"),
        (rect(23, 19, 29, 21), "metal"),
        (rect(24.5, 15, 26, 19.5), "dark"),
        (rect(8, 14, 29, 15.2), "edge", False),
        ([(12, 16), (21, 16), (20, 19), (11, 19)], "dark", False),
    ])
    g.part("slide", (20, 9), [
        (rect(5, 4, 37, 12.5, 1.5), "metal"),
        (rect(7, 5.2, 35, 6.6), "edge", False),
        (rect(28, 8, 31, 12), "dark", False),
        (rect(33, 2.5, 35.5, 4.2), "dark", False),
        (rect(8, 8.2, 14, 11.5), "accent", False),
    ])
    g.part("barrel", (37, 9), [(rect(36, 5.5, 43, 11.5), "dark"), (rect(37, 6.2, 42.5, 7.2), "edge", False)])
    return g.data()


def smg():
    g = Gun("smg", "SMG", (62, 38), vars_("#fbd484"),
            {"grip": [17, 22], "support": [41, 27], "muzzle": [60, 13], "eject": [34, 8], "mag": [32, 21]},
            stats(damage=10, fire_interval=0.07, auto=True, shot_speed=1000, spread=0.09, mag_size=30,
                  reload_time=1.5, range=260, recoil=3, zooms=[1, 1.25], melee=20))
    g.part("mag", (32, 21), [(rect(28, 20, 36, 36, 1), "dark"), (rect(27.5, 33.5, 36.5, 36), "accent"),
                             (rect(29.5, 21, 31, 33), "metal", False)])
    g.part("body", (17, 22), [
        (rect(3, 9, 9, 16, 1), "grip"),                          # folded stock
        (rect(8, 8, 46, 20, 2), "metal"),
        ([(12, 20), (23, 20), (20, 33), (10, 33)], "grip"),      # pistol grip
        (rect(23, 20, 30, 22), "metal"),
        (rect(24.5, 17, 26, 21.5), "dark"),
        (rect(46, 10, 56, 18), "dark"),                          # shroud
        (rect(10, 9.5, 44, 11), "edge", False),
        (rect(48, 12, 50, 17), "ink", False), (rect(52, 12, 54, 17), "ink", False),
        (rect(40, 14, 44, 18), "accent", False),
        (rect(14, 23, 19, 31), "dark", False),
    ])
    g.part("bolt", (30, 11), [(rect(24, 5, 37, 9, 1), "dark"), (rect(25, 5.8, 36, 6.8), "edge", False)])
    g.part("barrel", (56, 13), [(rect(55, 10.5, 61, 15.5), "metal"), (rect(55.5, 11.2, 60.5, 12.2), "edge", False)])
    return g.data()


def rifle():
    g = Gun("rifle", "Rifle", (100, 42), vars_("#e99563"),
            {"grip": [33, 24], "support": [57, 28], "muzzle": [98, 14], "eject": [48, 10], "mag": [43, 22]},
            stats(damage=16, fire_interval=0.12, auto=True, shot_speed=1200, spread=0.035, mag_size=30,
                  reload_time=1.9, range=520, recoil=5, zooms=[1, 1.5, 2], melee=30))
    curve = [(38, 21)] + arc((38, 21), (46, 37), (-3, 5)) + [(46, 37), (54, 36)] + \
        list(reversed(arc((54, 36), (47, 21), (0, 5)))) + [(47, 21)]
    g.part("mag", (43, 22), [(curve, "dark"), (rect(47, 35.5, 53, 37.5), "accent", False)])
    g.part("body", (33, 24), [
        ([(4, 10), (24, 11), (24, 23), (12, 25), (4, 20)], "wood"),     # stock
        (rect(22, 9, 60, 22, 1.5), "metal"),                             # receiver
        ([(28, 22), (38, 22), (36, 35), (26, 35)], "grip"),
        (rect(38, 22, 44, 24), "metal"),
        (rect(40, 18, 41.5, 22.5), "dark"),
        (rect(51, 10, 82, 21, 1.5), "dark"),                             # handguard
        (rect(24, 10.5, 58, 12), "edge", False),
        (rect(53, 11.5, 80, 12.5), "metal", False),
        (rect(36, 5, 50, 9), "dark"),                                    # rear sight block
        (rect(72, 6, 75, 10), "dark"),                                   # front sight post
        (rect(6, 13, 9, 19), "dark", False),
        (rect(54, 16, 60, 20), "accent", False),
    ])
    g.part("bolt", (50, 12), [(rect(44, 12, 56, 16, 1), "accent"), (rect(45, 12.8, 55, 13.6), "gold", False)])
    g.part("barrel", (82, 14), [
        (rect(81, 12, 95, 16), "metal"), (rect(94, 10, 99, 18, 1), "dark"), (rect(82, 12.6, 94, 13.4), "edge", False)])
    return g.data()


def shotgun():
    g = Gun("shotgun", "Shotgun", (104, 36), vars_("#e0584a"),
            {"grip": [33, 21], "support": [57, 28], "muzzle": [102, 10], "eject": [46, 9], "mag": [44, 21]},
            stats(damage=14, fire_interval=0.9, auto=False, shot_speed=800, spread=0.14, pellets=8,
                  mag_size=6, reload_time=2.4, range=170, recoil=9, zooms=[1, 1.25], melee=35))
    g.part("mag", (44, 21), [(rect(38, 20, 52, 27, 1.5), "dark"),
                             (rect(39.5, 21, 42, 26), "accent", False), (rect(43.5, 21, 46, 26), "accent", False),
                             (rect(47.5, 21, 50, 26), "accent", False)])
    g.part("body", (33, 21), [
        ([(4, 8), (28, 10), (28, 21), (14, 24), (4, 20)], "wood"),
        (rect(26, 8, 54, 20, 1.5), "metal"),
        ([(27, 20), (37, 20), (35, 31), (25, 31)], "grip"),
        (rect(37, 20, 43, 22), "metal"),
        (rect(38.5, 17, 40, 21.5), "dark"),
        (rect(54, 14, 90, 19), "dark"),                                  # magazine tube
        (rect(28, 9.5, 52, 11), "edge", False),
        (rect(30, 13, 38, 17), "accent", False),
    ])
    g.part("slide", (59, 17), [                                          # pump forend
        (rect(49, 14, 71, 23, 2), "wood"), (rect(51, 15.2, 69, 16.4), "gold", False),
        (rect(53, 19, 55, 22), "grip", False), (rect(57, 19, 59, 22), "grip", False), (rect(61, 19, 63, 22), "grip", False)])
    g.part("barrel", (54, 10), [
        (rect(53, 7, 101, 13), "metal"), (rect(54, 7.8, 100, 8.8), "edge", False), (rect(97, 4, 99, 7), "accent")])
    return g.data()


def sniper():
    g = Gun("sniper", "Sniper", (132, 40), vars_("#fbd484"),
            {"grip": [40, 24], "support": [64, 29], "muzzle": [130, 17], "eject": [58, 14], "mag": [52, 24]},
            stats(damage=80, fire_interval=1.5, auto=False, shot_speed=2200, spread=0.002, mag_size=5,
                  reload_time=2.8, range=1400, recoil=10, zooms=[1, 2, 3, 4], melee=35))
    g.part("mag", (52, 24), [(rect(47, 23, 57, 32, 1), "dark"), (rect(46.5, 30, 57.5, 32.5), "accent", False)])
    g.part("body", (40, 24), [
        ([(4, 14), (30, 13), (32, 24), (16, 26), (11, 32), (4, 32)], "wood"),   # stock with cheek and butt
        (rect(30, 12, 66, 24, 1.5), "metal"),                                  # receiver
        ([(36, 24), (46, 24), (43, 36), (34, 36)], "grip"),
        (rect(46, 24, 52, 26), "metal"),
        (rect(48, 20, 49.5, 24.5), "dark"),
        (rect(58, 19, 92, 24), "wood"),                                        # fore-end
        (rect(32, 13.5, 64, 15), "edge", False),
        (rect(7, 17, 9, 29), "dark", False),
        # scope: mounts and tube
        (rect(44, 9, 48, 12.5), "dark"), (rect(64, 9, 68, 12.5), "dark"),
        (rect(40, 3, 74, 9, 2.5), "dark"),
        (rect(37, 2, 42, 10, 1.5), "metal"), (rect(72, 1.5, 78, 10.5, 1.5), "metal"),
        (rect(43, 4, 70, 5.2), "edge", False),
        (rect(74, 3, 75.5, 8), "accent", False), (rect(38.5, 3.5, 40, 8.5), "accent", False),
    ])
    g.part("bolt", (56, 17), [(rect(50, 15.5, 62, 17.8), "metal"), (rect(60, 13, 65, 20, 1.5), "accent")])
    g.part("barrel", (92, 17), [
        (rect(91, 15, 124, 19), "metal"), (rect(92, 15.6, 123, 16.4), "edge", False),
        (rect(122, 13, 130, 21, 1.5), "dark"), (rect(124, 14, 125, 20), "metal", False)])
    return g.data()


def rocket():
    g = Gun("rocket", "Rocket launcher", (116, 42), vars_("#e99563"),
            {"grip": [48, 24], "support": [72, 31], "muzzle": [111, 16], "eject": [6, 16], "mag": [96, 16]},
            stats(damage=80, fire_interval=1.6, auto=False, shot_speed=480, spread=0.01, mag_size=2,
                  reload_time=3.0, range=900, recoil=8, kind="rocket", splash=70, zooms=[1, 1.5, 2], melee=40))
    g.part("mag", (96, 16), [([(94, 10), (106, 11), (111, 16), (106, 21), (94, 22)], "accent"),
                             (rect(95, 11.5, 105, 12.7), "gold", False)])
    g.part("body", (48, 24), [
        ([(5, 5), (14, 8), (14, 24), (5, 27)], "dark"),                      # rear flare
        (rect(12, 8, 96, 24, 3), "metal"),                                   # tube
        (rect(90, 5, 100, 27, 2), "dark"),                                   # front lip
        ([(43, 24), (55, 24), (52, 38), (41, 38)], "grip"),                  # trigger grip
        (rect(55, 24, 62, 26), "metal"), (rect(57, 21, 58.5, 25.5), "dark"),
        (rect(14, 9.5, 88, 11.2), "edge", False),
        (rect(26, 14, 34, 22), "accent", False), (rect(36, 14, 38, 22), "dark", False), (rect(40, 14, 42, 22), "dark", False),
        (rect(60, 2, 78, 8, 1.5), "dark"), (rect(62, 3, 76, 4.2), "edge", False),   # sight
        (rect(8, 10, 10, 22), "ink", False),
    ])
    return g.data()


def main():
    data = {"info": {"version": 1}, "guns": [pistol(), smg(), rifle(), shotgun(), sniper(), rocket()]}
    path = ROOT / "demo_apps/jetpack/baked/guns.lua"
    text = source("jetpack-guns", data).replace(
        "-- Baked by scripts/bake_jetpack.py from the workspace asset", "-- Generated by scripts/gen_guns.py (bake_jetpack.source) as the workspace asset", 1)
    path.write_text(text)
    print(f"wrote {path}")


if __name__ == "__main__":
    main()
