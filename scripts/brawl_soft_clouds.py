"""Writes demo_apps/brawl/art/cloud_soft_2.svg and cloud_soft_3.svg: the same recipe as the hand-built
cloud_soft_1.svg (gfx.svg has no blur, so softness is rings of the same lobes: three faint white rings,
a pale core, two highlight passes, each ring one nonzero path so overlaps fill once).

    python3 -I scripts/brawl_soft_clouds.py

Each cloud is lobes (cx, cy, r) plus one base ellipse (cx, cy, rx, ry). The anchor, the bottom centre of
the core (the ellipse's bottom), goes in maps/assets/clouds.lua. Agent-drawn, CC0 like the pack."""

from pathlib import Path

ART = Path(__file__).resolve().parent.parent / "demo_apps" / "brawl" / "art"

CLOUDS = {
    "cloud_soft_2": dict(size=(520, 200), lobes=[(110, 120, 44), (180, 100, 58), (270, 86, 66), (360, 104, 56), (430, 124, 42), (250, 126, 56)],
                         base=(265, 140, 190, 36), lights=[(210, 84, 40), (300, 80, 44), (150, 108, 26)]),
    "cloud_soft_3": dict(size=(360, 210), lobes=[(100, 126, 40), (170, 100, 56), (250, 112, 48), (300, 138, 34), (180, 138, 50)],
                         base=(200, 150, 140, 28), lights=[(176, 82, 36), (246, 96, 28), (104, 112, 24)]),
}


def circle(cx, cy, r):
    return f"M{cx - r} {cy} A{r} {r} 0 1 0 {cx + r} {cy} A{r} {r} 0 1 0 {cx - r} {cy} Z"


def ellipse(cx, cy, rx, ry):
    return f"M{cx - rx} {cy} A{rx} {ry} 0 1 0 {cx + rx} {cy} A{rx} {ry} 0 1 0 {cx - rx} {cy} Z"


def ring(c, grow, fill, opacity):
    d = " ".join(circle(x, y, r + grow) for x, y, r in c["lobes"]) + " " + ellipse(c["base"][0], c["base"][1], c["base"][2] + grow, c["base"][3] + grow)
    return f'    <path fill="{fill}" fill-opacity="{opacity}" d="{d}"/>'


def light(c, scale, opacity):
    d = " ".join(circle(x, y, round(r * scale)) for x, y, r in c["lights"])
    return f'    <path fill="#ffffff" fill-opacity="{opacity}" d="{d}"/>'


def svg(c):
    w, h = c["size"]
    rows = [ring(c, 18, "#ffffff", 0.12), ring(c, 11, "#ffffff", 0.16), ring(c, 5, "#ffffff", 0.22), ring(c, 0, "#dbe7ef", 0.85),
            light(c, 1.0, 0.8), light(c, 0.7, 0.6)]
    return (f'<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">\n'
            "  <!-- A soft day cloud for Brawl (scripts/brawl_soft_clouds.py): rings of translucent lobes, no blur. -->\n"
            '  <g id="cloud">\n' + "\n".join(rows) + "\n  </g>\n</svg>\n")


for name, c in CLOUDS.items():
    (ART / f"{name}.svg").write_text(svg(c), encoding="utf-8")
    x, y, rx, ry = c["base"]
    print(f"{name}.svg {c['size'][0]}x{c['size'][1]} anchor ({x}, {y + ry})")
