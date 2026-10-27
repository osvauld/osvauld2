"""Generate Jetpack's baked maps (design/jetpack-militia.md "Maps"): three big arenas and the small
training yard, written to baked/maps.lua in the game and the map editor (identical copies).

    python3 scripts/gen_maps.py

The yard is data here (it was hand-made in the map editor); `bake_jetpack.py` rewrites maps.lua from the
editor's asset, so re-run this only to regenerate the arenas. `theme`, `loadout` (the guns everyone starts with:
in hand, then on the back) and `weapon_spawns` (a `pool` of gun ids, a gun listed twice twice as likely, and a
`respawn` in seconds) are optional keys of `F.map`; the rules read them from step 5.
Reach numbers (theme.lua): gravity 720, thrust 1120 -> net climb accel 400, max_rise 190, fuel 100 at
32/s = 3.1 s of thrust (one tank climbs ~550 units), refill 23/s standing. Hops here stay <= 420 up.
"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from bake_jetpack import source

ROOT = Path(__file__).resolve().parent.parent
RY = 19  # collider half-height: a standing soldier's centre is RY above the surface


class Map:
    def __init__(self, id, name, w, h, theme):
        self.id, self.name, self.w, self.h, self.theme = id, name, w, h, theme
        self.platforms, self.spawns, self.weapons, self.n = [], [], [], {}

    def _id(self, p):
        self.n[p] = self.n.get(p, 0) + 1
        return f"{p}{self.n[p]}"

    def _add(self, p, kind, x, top, w, h):
        self.platforms.append({"id": self._id(p), "kind": kind, "x": x, "y": top, "w": w, "h": h})

    def ground(self, x, top, w, bottom=None):
        self._add("g", "ground", x, top, w, (bottom if bottom is not None else self.h) - top)

    def ledge(self, x, top, w, h=18):
        self._add("l", "ledge", x, top, w, h)

    def cover(self, x, base, w, h):  # sits on a surface whose top is `base`
        self._add("c", "cover", x, base - h, w, h)

    def spawn(self, team, x, base):
        self.spawns.append({"id": self._id("sp"), "team": team, "x": x, "y": base - RY})

    def weapon(self, x, base, pool, respawn):
        self.weapons.append({"id": self._id("w"), "x": x, "y": base - 14, "pool": pool, "respawn": respawn})

    def floor(self, top, holes, thick=40, x0=0, x1=None, kind="ground"):
        """A full-width floor strip with `holes` [(x0, x1)] cut out (shafts)."""
        x1 = x1 or self.w
        edges, cur = [], x0
        for a, b in sorted(holes):
            if a > cur:
                edges.append((cur, a))
            cur = b
        if cur < x1:
            edges.append((cur, x1))
        for a, b in edges:
            self._add("g" if kind == "ground" else "l", kind, a, top, b - a, thick)

    def out(self):
        return {"id": self.id, "name": self.name, "w": self.w, "h": self.h, "theme": self.theme,
                "loadout": ["rifle", "pistol"],
                "platforms": self.platforms, "spawns": self.spawns, "weapon_spawns": self.weapons}


def outpost():
    m = Map("outpost", "Cliffside Outpost", 4400, 2000, "cliffs")
    m.ground(0, 1900, 4400, 2000)                       # valley floor
    # left cliff, three 300-unit terraces
    m.ground(0, 1450, 640, 1900); m.ground(0, 1150, 380, 1450); m.ground(0, 850, 260, 1150)
    # right cliff, wider and rougher
    m.ground(3700, 1500, 700, 1900); m.ground(4020, 1200, 380, 1500); m.ground(4140, 900, 260, 1200)
    # central spire
    m.ground(2090, 1350, 240, 1900)
    # outposts: watchtower roof + sandbags on the left, bunker wall on the right
    m.ledge(400, 1300, 160); m.cover(430, 1450, 40, 90); m.cover(580, 1450, 40, 60)
    m.cover(3780, 1500, 50, 90); m.cover(3960, 1500, 60, 70)
    m.cover(4160, 900, 70, 50); m.cover(40, 850, 60, 55)
    # valley cover and the pillboxes that make the floor a fight
    for x, w, h in [(700, 90, 70), (1180, 60, 90), (1650, 120, 55), (2500, 120, 55), (2950, 60, 90), (3400, 90, 70)]:
        m.cover(x, 1900, w, h)
    # lower islands, left to right (each hop <= 300 up, gaps <= 330)
    for x, y, w in [(620, 1700, 180), (760, 1560, 200), (1060, 1480, 200), (1340, 1330, 240), (1000, 1180, 220),
                    (1330, 1040, 260), (700, 1000, 200), (1650, 1200, 200),
                    (2560, 1700, 200), (2560, 1500, 200), (2820, 1340, 240), (3120, 1480, 200), (3350, 1180, 240),
                    (2900, 1040, 220), (3480, 1330, 200), (2950, 1680, 180)]:
        m.ledge(x, y, w)
    # spire crown and the chain up to the bridge
    m.ledge(2000, 1130, 420); m.ledge(2100, 880, 220)
    m.cover(2150, 1130, 60, 70); m.cover(2260, 1130, 60, 50)
    # upper left approach: summit -> hops -> bridge; and the high bridge with its cover
    m.ledge(480, 700, 220); m.ledge(850, 560, 200); m.ledge(1250, 470, 250)
    m.ledge(1500, 800, 300)
    m.ledge(1700, 560, 1500, 24)
    for x, w, h in [(1900, 70, 60), (2400, 100, 70), (2850, 60, 60)]:
        m.cover(x, 560, w, h)
    m.ledge(2700, 760, 300)
    m.ledge(3450, 660, 250); m.ledge(3800, 760, 220)
    m.ledge(900, 300, 160); m.ledge(3500, 300, 160); m.ledge(2150, 330, 160)
    # spawns
    m.spawn("player", 500, 1450)
    m.spawn("player", 200, 850); m.spawn("player", 850, 1900)
    for x, b in [(4250, 900), (4080, 1200), (2380, 1130), (2650, 560), (3200, 1900), (3900, 1500), (1800, 560), (2700, 1900)]:
        m.spawn("bot", x, b)
    # weapons: sniper on the summits, rocket on the spire crown, shotguns for the valley
    m.weapon(200, 850, ["sniper"], 60); m.weapon(4270, 900, ["sniper"], 60)
    m.weapon(2030, 1130, ["rocket"], 75)
    m.weapon(1760, 560, ["rifle", "smg"], 25); m.weapon(2750, 560, ["rifle", "smg"], 25)
    m.weapon(1160, 1900, ["shotgun"], 30); m.weapon(3150, 1900, ["shotgun"], 30)
    m.weapon(1430, 1330, ["smg", "pistol"], 20); m.weapon(3440, 1180, ["rifle"], 30)
    m.weapon(520, 1450, ["rifle"], 30); m.weapon(3880, 1500, ["smg"], 25)
    m.weapon(1100, 1180, ["pistol", "smg"], 20)
    return m


def catacombs():
    m = Map("catacombs", "The Catacombs", 4000, 2200, "underground")
    levels = [2100, 1800, 1500, 1200, 900, 600, 300]
    holes = {
        1800: [(480, 760), (1800, 2100), (3200, 3480)],
        1500: [(1100, 1400), (2700, 3000)],
        1200: [(300, 580), (2000, 2300), (3400, 3700)],
        900: [(900, 1200), (1450, 2450), (2800, 3100)],
        600: [(200, 480), (1500, 1800), (3000, 3300)],
        300: [(800, 1100), (1900, 2200), (2900, 3200)],
    }
    m.ground(0, 2100, 4000, 2200)
    for y in levels[1:]:
        m.floor(y, holes[y])
    # arches hang from the floor above and leave a 120 doorway: they split each level into chambers
    arch = {1800: [1000, 2500, 3700], 1500: [500, 1800, 3300], 1200: [900, 2700], 900: [350, 3500],
            600: [700, 2300, 3700]}
    for y, xs in arch.items():
        for x in xs:
            m.ground(x, y - 260, 40, y - 120)
    # rock mounds on the floors and stalactites under the ceilings, so tunnels are not tidy
    for x, y, w in [(1000, 2100, 240), (1500, 1800, 220), (1600, 1500, 200), (2800, 1200, 200), (1150, 600, 240), (2500, 900, 250)]:
        m.ground(x, y - 70, w, y)
    for y, xs in {2100: [400, 1500, 3000], 1800: [200, 2000, 3600], 1500: [1000, 2600], 1200: [300, 3300],
                  600: [500, 1500, 3500]}.items():
        for x in xs:
            m.ground(x, y - 260, 180, y - 170)
    # hall balconies in the double-height crypt (L900 cut away between 1450 and 2450)
    m.ledge(1480, 1050, 220); m.ledge(2230, 880, 220); m.ledge(1800, 960, 300)
    # floor furniture: pillars (cover 90), sarcophagi (50), per level
    furn = {2100: [(300, 60, 90), (800, 120, 50), (1400, 60, 90), (1900, 150, 50), (2600, 60, 90), (3300, 120, 50), (3700, 60, 90)],
            1800: [(120, 120, 50), (1300, 60, 90), (2300, 120, 50), (2900, 60, 90), (3600, 60, 90)],
            1500: [(300, 120, 50), (800, 60, 90), (2200, 60, 90), (3100, 120, 50), (3700, 60, 90)],
            1200: [(100, 60, 90), (1000, 120, 50), (1500, 60, 90), (2600, 120, 50), (3200, 60, 90)],
            900: [(100, 60, 90), (600, 120, 50), (3200, 120, 50), (3700, 60, 90)],
            600: [(900, 60, 90), (2000, 120, 50), (2600, 60, 90), (3500, 120, 50)]}
    for y, items in furn.items():
        for x, w, h in items:
            m.cover(x, y, w, h)
    # surface graveyard: tombstones, a ruined chapel roof and a lookout
    for x, w, h in [(200, 60, 70), (500, 60, 70), (1300, 100, 60), (1500, 60, 70), (2500, 100, 60), (3400, 60, 70), (3700, 60, 70)]:
        m.cover(x, 300, w, h)
    m.ledge(1300, 130, 300); m.ledge(3000, 130, 260); m.ledge(2000, 60, 160)
    # spawns
    m.spawn("player", 200, 2100); m.spawn("player", 1600, 2100); m.spawn("player", 3800, 2100)
    for x, b in [(3500, 1500), (3700, 1800), (1200, 1200), (3600, 900), (1000, 600), (3300, 300), (400, 1800), (2200, 1800)]:
        m.spawn("bot", x, b)
    m.weapon(560, 2100, ["shotgun"], 25); m.weapon(3550, 2100, ["shotgun"], 25)
    m.weapon(2200, 1800, ["smg"], 20); m.weapon(1000, 1500, ["rifle", "smg"], 25)
    m.weapon(3120, 1200, ["shotgun", "smg"], 25)
    m.weapon(1900, 960, ["rocket"], 75)
    m.weapon(3450, 900, ["rifle"], 30); m.weapon(500, 900, ["pistol", "smg"], 20)
    m.weapon(2400, 600, ["rifle"], 30); m.weapon(3800, 600, ["smg"], 20)
    m.weapon(1450, 130, ["sniper"], 60); m.weapon(3150, 130, ["sniper"], 60)
    m.weapon(3300, 1500, ["pistol"], 15)
    return m


def rooftops():
    m = Map("rooftops", "Ruined Rooftops", 4800, 2200, "city")
    street = 2060
    m.ground(0, street, 4800, 2200)
    # buildings: (x, w, roof top)
    b = [(0, 520, 1500), (780, 640, 1000), (1700, 560, 1650), (2700, 600, 850), (3560, 560, 1350), (4360, 440, 1100)]
    for x, w, roof in b:
        if x == 1700:  # ruined shell: doorways, a broken roof, an interior floor
            m.ground(1700, roof, 60, 1930); m.ground(1760, roof, 190, roof + 40)
            m.ground(2100, roof, 100, roof + 40); m.ground(2200, roof, 60, 1930)
        else:
            m.ground(x, roof, w, street)
    # penthouses, water tanks and stair blocks on the roofs
    for x, w, roof, hh in [(200, 160, 1500, 90), (1360, 60, 1000, 60), (3200, 100, 850, 100),
                           (4050, 60, 1350, 90), (4700, 90, 1100, 80)]:
        m.ground(x, roof - hh, w, roof)
    # fire escapes in the alleys, zig-zag up each face (each hop <= 200 up)
    for x, y, w in [(560, 1330, 160), (610, 1170, 140), (3330, 1230, 150), (3380, 1090, 150),
                    (1450, 1480, 200), (2320, 1480, 180), (2340, 1300, 160), (2500, 1120, 180),
                    (4160, 1230, 160), (4180, 1080, 150),
                    (1440, 1850, 120), (520, 1750, 220), (3320, 1850, 200), (4150, 1850, 160), (1760, 1850, 440)]:
        m.ledge(x, y, w)
    # open ruined floors: slabs inside the low shell (B3) and gaps in the plaza
    m.ledge(2290, 1800, 300); m.ledge(2290, 1580, 160)
    # plaza: fallen bridge, crane arms, drone lanes above
    m.ledge(2640, 1250, 120)
    m.ledge(900, 640, 360, 24); m.ledge(1500, 760, 320); m.ledge(2100, 640, 400, 24)
    m.ledge(3200, 620, 400, 24); m.ledge(3800, 780, 300); m.ledge(4300, 700, 260)
    m.ledge(400, 1150, 240); m.ledge(250, 880, 220); m.ledge(2500, 410, 200); m.ledge(1100, 400, 260)
    m.ledge(3900, 440, 240)
    # roof cover and billboards
    for x, base, w, h in [(40, 1500, 80, 60), (380, 1500, 70, 80),
                          (850, 1000, 60, 80), (1250, 1000, 90, 60),
                          (1800, 1650, 90, 70), (2100, 1650, 70, 90),
                          (2780, 850, 80, 80), (3100, 850, 70, 60),
                          (3640, 1350, 90, 70), (3980, 1350, 60, 80),
                          (4440, 1100, 90, 70)]:
        m.cover(x, base, w, h)
    # street barricades and wrecks
    for x, w, h in [(580, 70, 90), (1550, 100, 60), (2300, 120, 55), (2600, 70, 90), (3380, 70, 90), (4200, 100, 60)]:
        m.cover(x, street, w, h)
    m.spawn("player", 680, street); m.spawn("player", 480, 1500); m.spawn("player", 2450, street)
    for x, b2 in [(4600, 1100), (3800, 1350), (3000, 850), (1100, 1000), (1925, 1650), (4150, street), (1480, street), (2330, 1800)]:
        m.spawn("bot", x, b2)
    m.weapon(3000, 850, ["sniper"], 60); m.weapon(1000, 640, ["sniper"], 60)
    m.weapon(2520, 1800, ["rocket"], 75)
    m.weapon(730, street, ["shotgun"], 25); m.weapon(3500, street, ["shotgun"], 25)
    m.weapon(1100, 1000, ["rifle", "smg"], 25); m.weapon(3800, 1350, ["rifle", "smg"], 25)
    m.weapon(150, 1500, ["pistol", "smg"], 20); m.weapon(4650, 1100, ["rifle"], 30)
    m.weapon(2300, 640, ["smg"], 20); m.weapon(3300, 620, ["shotgun", "rifle"], 25)
    m.weapon(1778, 1650, ["smg"], 20); m.weapon(2520, street, ["pistol"], 15)
    return m


YARD = {
    "id": "yard", "name": "Training Yard", "w": 2880, "h": 480, "theme": "yard",
    "platforms": [
        {"id": "ground", "kind": "ground", "x": 0, "y": 450, "w": 2880, "h": 30},
        {"id": "p1", "kind": "ledge", "x": 210, "y": 322, "w": 166, "h": 18},
        {"id": "p2", "kind": "ledge", "x": 575, "y": 290, "w": 175, "h": 18},
        {"id": "p3", "kind": "ledge", "x": 378, "y": 173, "w": 200, "h": 18},
        {"id": "c1", "kind": "cover", "x": 456, "y": 370, "w": 48, "h": 80},
        {"id": "p4", "kind": "ledge", "x": 970, "y": 335, "w": 210, "h": 18},
        {"id": "p5", "kind": "ledge", "x": 1230, "y": 235, "w": 180, "h": 18},
        {"id": "p6", "kind": "ledge", "x": 1510, "y": 345, "w": 210, "h": 18},
        {"id": "p7", "kind": "ledge", "x": 1620, "y": 165, "w": 190, "h": 18},
        {"id": "c2", "kind": "cover", "x": 1810, "y": 355, "w": 70, "h": 95},
        {"id": "p8", "kind": "ledge", "x": 2040, "y": 290, "w": 220, "h": 18},
        {"id": "p9", "kind": "ledge", "x": 2300, "y": 185, "w": 180, "h": 18},
        {"id": "p10", "kind": "ledge", "x": 2560, "y": 330, "w": 200, "h": 18},
    ],
    "spawns": [
        {"id": "you", "team": "player", "x": 135, "y": 431},
        {"id": "bot1", "team": "bot", "x": 705, "y": 150},
        {"id": "bot2", "team": "bot", "x": 825, "y": 225},
    ],
}


def main():
    maps = [f().out() for f in (outpost, catacombs, rooftops)] + [YARD]
    text = source("jetpack-maps", {"info": {"version": 1}, "maps": maps})
    for app in ("jetpack", "jetpack_map_editor"):
        out = ROOT / "demo_apps" / app / "baked/maps.lua"
        out.write_text(text)
        print(out)
    print([(m["id"], len(m["platforms"]), len(m["spawns"]), len(m.get("weapon_spawns", []))) for m in maps])


if __name__ == "__main__":
    main()
