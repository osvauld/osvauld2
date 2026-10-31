"""Jetpack firing animation over the real path (design/jetpack-militia.md step 3, test 3): frame-stepped
screenshots (the virtual clock, 1/60 s per frame) around one shot show the muzzle flash at the model's muzzle
(placed from the game's camera readout and the gun's own points, never guessed), a flash per gun kind (small
pistol, long rifle, wide shotgun, rocket back-blast), the tracer leaving the muzzle, a casing at two different
positions in two frames (none for the rocket; a red shell for the shotgun), the rocket and its blast, the same
mirrored facing left, and the flash gone a tenth of a second later. Leaves shots/jetpack-fire-<gun>[-left].png:
a sheet of the frames around the shot, to judge the look by."""
import base64
import re
import shutil
import struct
import sys
import tempfile
import zlib
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from osvauld.session import Session, shell_binary
from osvauld.png import Image, near

import os
FAILS = []


def check(ok, why):
    """JETPACK_FIRE_SOFT=1 collects failures and goes on (to look at the sheets), else the first one stops."""
    if not ok:
        if not os.environ.get("JETPACK_FIRE_SOFT"):
            raise AssertionError(why)
        FAILS.append(why)
        print("CHECK FAILED:", why)


ROOT = Path(__file__).resolve().parent.parent
FLASH, FLASH_CORE = "#ff9a2e", "#fffbd6"
BRASS, SHELL, TRACER = "#f0c030", "#d8392f", "#ffffff"
RING, NOSE = "#ffd27a", "#e0523c"
FRAMES = 8  # frames of the sheet: the one holding the shot, then seven more

_pilot = (ROOT / "demo_apps/jetpack/baked/pilots.lua").read_text()
HOLD = [float(v) for v in re.search(r"hold = \{ ([\d.]+), ([\d.]+) \}", _pilot).groups()]
SIZE = [float(v) for v in re.search(r"\bsize = \{ ([\d.]+), ([\d.]+) \}", _pilot).groups()]
SCALE = float(re.search(r"\bscale = ([\d.]+)", _pilot).group(1))
ACTOR_W = SIZE[0] * SCALE
ANCHOR = (ACTOR_W / 2, SIZE[1] * SCALE - 24)

_guns = (ROOT / "demo_apps/jetpack/baked/guns.lua").read_text()


def gun_data(identity):
    m = re.search(r'id = "%s",.*?points = \{(.*?)\},\s*scale = ([\d.]+)' % identity, _guns, re.S)
    points = {k: (float(x), float(y)) for k, x, y in re.findall(r"(\w+) = \{ ([\d.]+), ([\d.]+) \}", m.group(1))}
    return points, float(m.group(2))


def find(tree, identity):
    if tree.get("id") == identity:
        return tree
    for child in tree.get("children", []):
        found = find(child, identity)
        if found:
            return found


def actor_point(gun, name, facing):
    """A gun point in the actor's frame (game units) at a level aim, from the gun's own numbers and the scout's hold."""
    points, k = gun_data(gun)
    grip = (HOLD[0] * SCALE, HOLD[1] * SCALE) if facing > 0 else ((SIZE[0] - HOLD[0]) * SCALE, HOLD[1] * SCALE)
    p, g = points[name], points["grip"]
    return grip[0] + facing * (p[0] - g[0]) * k, grip[1] + (p[1] - g[1]) * k


BASE = [None]  # the screenshot before the shot: pixels already that colour there are scenery, not effects


def pixels(image, x0, y0, x1, y1, colour, tolerance=24):
    """Matching pixels in a screen box that the pre-shot screenshot did not already have, as (x, y) pairs."""
    out = []
    base = BASE[0]
    for y in range(max(0, int(y0)), min(image.height, int(y1) + 1)):
        for x in range(max(0, int(x0)), min(image.width, int(x1) + 1)):
            if near(image.at(x, y), colour, tolerance) and not (base and near(base.at(x, y), colour, tolerance)):
                out.append((x, y))
    return out


def sheet(crops, path, cols=4):
    """Tile (width, height, rows) crops into a PNG, `cols` to a row."""
    w, h = crops[0][0], crops[0][1]
    rows_n = (len(crops) + cols - 1) // cols
    canvas = [bytearray(b"\x10\x1c\x24\xff" * (w * cols)) for _ in range(h * rows_n)]
    for i, (_, _, rows) in enumerate(crops):
        ox, oy = (i % cols) * w, (i // cols) * h
        for y, line in enumerate(rows):
            canvas[oy + y][ox * 4:ox * 4 + len(line)] = line
    raw = b"".join(b"\x00" + bytes(line) for line in canvas)

    def chunk(kind, data):
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body))
    Path(path).write_bytes(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w * cols, h * rows_n, 8, 6, 0, 0, 0)) +
                           chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b""))


def crop(image, x, y, w, h):
    x, y, w, h = int(x), int(y), int(w), int(h)
    rows = []
    for yy in range(y, y + h):
        line = bytearray(b"\x00\x00\x00\xff" * w)
        if 0 <= yy < image.height:
            lo, hi = max(0, x), min(image.width, x + w)
            line[(lo - x) * 4:(hi - x) * 4] = image.rows[yy][lo * 4:hi * 4]
        rows.append(line)
    return w, h, rows


with Session(shell_binary=shell_binary(), offscreen=(1200, 900)) as s:
    r = s.rpc
    r.signup("gunner", "jetpack fire passphrase")
    ws = r.create_workspace("Jetpack fire")

    def app(name, folder):
        item = r.create_item(ws["id"], name, "app")["id"]
        r.upload_folder(item, ROOT / "demo_apps" / folder)
        r.open_item(item)
        r.frame(3)
        assert not r.read_console(item), r.read_console(item)
        return item

    def rect(identity):
        found = [e for e in r.rects() if e["id"] == identity]
        assert found, (identity, sorted(e["id"] for e in r.rects()))
        return found[0]

    def label(item, identity):
        node = find(r.dump_tree(item), identity)
        assert node, (identity, r.read_console(item))
        return node["text"]

    def shot(item):
        return Image(base64.b64decode(r.screenshot(item)["png_base64"]))

    def arm_with(gun):
        """The test-only loadout seam (jetpack-loadout asset): everyone holds `gun`."""
        seed_dir = Path(tempfile.mkdtemp(prefix="jetpack-loadout-"))
        (seed_dir / "manifest.osv").write_text('app "Seed loadout" {}\n')
        (seed_dir / "main.lua").write_text(
            "local a = asset:open('jetpack-loadout')\n"
            f"a:set({{ 'loadout' }}, doc.map({{ player = '{gun}', bot = '{gun}' }}))\n"
            "return function() return ui.col({ full = true }) end\n")
        seeder = r.create_item(ws["id"], "Seed " + gun, "app")["id"]
        r.upload_folder(seeder, seed_dir)
        r.open_item(seeder)
        r.frame(3)
        assert not r.read_console(seeder), r.read_console(seeder)
        shutil.rmtree(seed_dir)

    def settle(item, facing, down=0):
        """Aim level (or `down` game units below the gun) the way `facing` looks, re-aiming while the camera eases."""
        for _ in range(4):
            top = rect("player")
            cs = float(label(item, "camera:view").split()[2])  # px per game unit
            ax, ay = top["x"] + ANCHOR[0] * cs, top["y"] + ANCHOR[1] * cs
            r.move_to(max(8, ax + facing * 110 * cs), ay + (down - 4) * cs)  # not off the left edge of the view
            r.frame(30)
        top = rect("player")
        return top, float(label(item, "camera:view").split()[2])

    def fire(item, gun, facing, frames=FRAMES, down=0, extra=0):
        """Hold the trigger for one frame, then step `frames` frames in all (+ `extra` more), a screenshot each.
        Returns [(image, player rect, px per unit)] and the shots the HUD counted."""
        top, cs = settle(item, facing, down)
        before = int(label(item, "hud:fired").split()[-1])
        out = []
        BASE[0] = shot(item)
        r.keyboard("KeyF", "f", True)
        for i in range(frames + extra):
            r.frame(1)
            if i == 0:
                r.keyboard("KeyF", "f", False)
            top, cs = rect("player"), float(label(item, "camera:view").split()[2])
            out.append((shot(item), top, cs))
        return out, int(label(item, "hud:fired").split()[-1]) - before

    def box(top, cs, gun, name, facing, ahead=0.0, back=0.0, half=9.0):
        """A screen box around a gun point: `ahead` units forward of it, `back` behind, `half` above and below."""
        px, py = actor_point(gun, name, facing)
        x, y = top["x"] + px * cs, top["y"] + py * cs
        lo, hi = x - (back if facing > 0 else ahead) * cs, x + (ahead if facing > 0 else back) * cs
        return lo, y - half * cs, hi, y + half * cs

    def extent(found):
        if not found:
            return 0, 0
        # The flash is one blob; scenery that happens to match the flash colour is a stray run far from it (a camera
        # that sits a little differently found one): keep the biggest run of pixels with gaps under 15 px.
        pts, runs = sorted(found), []
        for p in pts:
            if runs and p[0] - runs[-1][-1][0] < 15:
                runs[-1].append(p)
            else:
                runs.append([p])
        found = max(runs, key=len)
        xs, ys = [p[0] for p in found], [p[1] for p in found]
        return max(xs) - min(xs), max(ys) - min(ys)

    metrics = {}

    def run(gun, facing, name, down=0, extra=4):
        item = app("Jetpack Arena", "jetpack")
        r.click_at(*r.centre_of("match:start"))
        r.frame(90)
        if facing < 0:  # the spawn is by the left wall: walk right so a left-facing shot has room
            r.keyboard("KeyD", "d", True)
            r.frame(100)
            r.keyboard("KeyD", "d", False)
            r.frame(40)
        tag = f"{gun} facing {facing}"
        # A screenshot costs the virtual clock a frame or two, so a shot's first sample can already be past the
        # flash; the clock is deterministic, so try again (the trigger lands on another step) a few times.
        for attempt in range(3):
            frames, shots = fire(item, gun, facing, down=down, extra=extra)
            assert shots == 1, f"{tag}: one frame of trigger should be one shot, counted {shots}"
            # The model's muzzle on screen, and the flash colour in a box around it, frame by frame.
            flash_px = []
            for image, top, cs in frames:
                m = box(top, cs, gun, "muzzle", facing, ahead=32, back=10, half=16)
                flash_px.append(pixels(image, *m, FLASH) + pixels(image, *m, FLASH_CORE, 14))
            counts = [len(p) for p in flash_px]
            seen = [i for i, c in enumerate(counts) if c >= 6]
            if seen and seen[0] <= 1:
                break
            print(f"{tag}: attempt {attempt}: flash pixels per frame {counts}")
            r.frame(150)
        print(f"{tag}: flash pixels per frame {counts}")
        check(seen and seen[0] <= 1, f"{tag}: no flash at the model's muzzle in the first frames ({counts})")
        check(gun == "rocket" or not any(counts[4:]), f"{tag}: the flash must be gone a few frames after the shot ({counts})")
        peak = max(range(len(frames)), key=lambda i: counts[i])
        w, h = extent(flash_px[peak])
        cs = frames[peak][2]
        metrics[(gun, facing)] = (w / cs, h / cs)
        # The flash sits ahead of the muzzle, in the facing direction (its centroid is on the muzzle's far side).
        image, top, cs = frames[peak]
        px, _ = actor_point(gun, "muzzle", facing)
        cx = sum(p[0] for p in flash_px[peak]) / max(1, len(flash_px[peak]))
        if gun != "rocket" and flash_px[peak]:
            check((cx - (top["x"] + px * cs)) * facing > 0, f"{tag}: the flash should point out of the muzzle")
        # The tracer: a white streak ahead of the muzzle in the next frames (bullets only).
        if gun not in ("rocket", "shotgun"):
            seen_tracer = 0
            for image, top, cs in frames[0:3]:
                far = box(top, cs, gun, "muzzle", facing, ahead=260, back=-30, half=14)
                seen_tracer = max(seen_tracer, len(pixels(image, *far, TRACER, 10)))
            check(seen_tracer >= 4, f"{tag}: no tracer streak after the shot ({seen_tracer})")
        # Casing: brass / red shell flying from the eject port (none for the rocket), at two positions in two frames.
        colour = SHELL if gun == "shotgun" else BRASS
        found = []
        for image, top, cs in frames:
            lo = top["x"] - 25 * cs
            hi = top["x"] + (ACTOR_W + 25) * cs
            got = pixels(image, lo, top["y"] - 25 * cs, hi, top["y"] + (SIZE[1] * SCALE + 12) * cs, colour, 20)
            if gun == "shotgun":  # the red shell: keep to the eject port's neighbourhood (the bot-colour is orange)
                got = [p for p in got if near(image.at(*p), SHELL, 20)]
            found.append([((x - top["x"]) / cs, (y - top["y"]) / cs) for x, y in got])
        sizes = [len(p) for p in found]
        print(f"{tag}: casing pixels per frame {sizes}")
        if gun == "rocket":
            far = lambda image, top, cs: pixels(image, top["x"] - 20 * cs, top["y"] - 90 * cs, top["x"] + 300 * cs, top["y"] + 90 * cs, NOSE, 14)
            nose = max(len(far(*f)) for f in frames[:6])
            print(f"{tag}: rocket nose pixels {nose}")
            check(nose >= 4, f"{tag}: the rocket is drawn in flight ({nose})")
            check(max(sizes) == 0, f"{tag}: a rocket ejects no casing ({sizes})")
        else:
            live = [i for i, p in enumerate(found) if len(p) >= 3]
            check(len(live) >= 2, f"{tag}: no casing seen in two frames ({sizes})")
            if len(live) >= 2:
                a_, b_ = found[live[0]], found[live[-1]]
                ca = (sum(p[0] for p in a_) / len(a_), sum(p[1] for p in a_) / len(a_))
                cb = (sum(p[0] for p in b_) / len(b_), sum(p[1] for p in b_) / len(b_))
                moved = ((ca[0] - cb[0]) ** 2 + (ca[1] - cb[1]) ** 2) ** 0.5
                check(moved > 1.5, f"{tag}: the casing did not move between frames {live[0]} and {live[-1]} ({ca} -> {cb})")
                # It leaves backwards: its x is on the back side of the muzzle, up first.
                print(f"{tag}: casing at {ca[0]:.1f},{ca[1]:.1f} -> {cb[0]:.1f},{cb[1]:.1f} game units (actor frame)")
        # The sheet: the frames around the shot, cropped round the soldier.
        crops = []
        for image, top, cs in frames[:FRAMES]:
            x0 = top["x"] - (35 if facing > 0 else 53) * cs
            crops.append(crop(image, x0, top["y"] - 20 * cs, 140 * cs, 85 * cs))
        sheet(crops, ROOT / "shots" / name)
        assert not r.read_console(item), r.read_console(item)
        r.request("CloseItem", item_id=item)
        r.frame(2)
        return frames

    # The baked rifle (no loadout asset yet): right and left.
    run("rifle", 1, "jetpack-fire-rifle.png")
    run("rifle", -1, "jetpack-fire-rifle-left.png")
    arm_with("pistol")
    run("pistol", 1, "jetpack-fire-pistol.png")
    arm_with("shotgun")
    run("shotgun", 1, "jetpack-fire-shotgun.png")
    arm_with("rocket")
    frames = run("rocket", 1, "jetpack-fire-rocket.png", down=36)
    run("rocket", -1, "jetpack-fire-rocket-left.png", down=36)

    # Flash by kind: the rifle's is longer than the pistol's, the shotgun's is wider.
    rw, pw, sh = metrics[("rifle", 1)], metrics[("pistol", 1)], metrics[("shotgun", 1)]
    print(f"flash extent (w x h game units): pistol {pw[0]:.0f}x{pw[1]:.0f}, rifle {rw[0]:.0f}x{rw[1]:.0f}, shotgun {sh[0]:.0f}x{sh[1]:.0f}")
    check(rw[0] > pw[0] * 1.3, ("rifle flash should be longer than the pistol's", rw, pw))
    check(sh[1] > pw[1] * 1.6 and sh[1] > rw[1] * 1.6, ("shotgun flash should be wider", sh, pw, rw))
    # Mirror: left and right flashes are about the same size.
    for gun in ("rifle", "rocket"):
        a_, b_ = metrics[(gun, 1)], metrics[(gun, -1)]
        check(abs(a_[0] - b_[0]) < 6 and abs(a_[1] - b_[1]) < 6, (gun, "left flash is the mirror of right", a_, b_))

    # The rocket in flight and its blast: fire at the ground ahead and watch.
    item = app("Jetpack Arena", "jetpack")
    r.click_at(*r.centre_of("match:start"))
    r.frame(90)
    settle(item, 1, down=36)
    r.keyboard("KeyF", "f", True)
    r.frame(1)
    r.keyboard("KeyF", "f", False)
    best, blast_crops = 0, []
    r.frame(6)
    for i in range(24):
        r.frame(2)
        top, cs = rect("player"), float(label(item, "camera:view").split()[2])
        image = shot(item)
        near_player = (top["x"] - 20 * cs, top["y"] - 90 * cs, top["x"] + 300 * cs, top["y"] + 90 * cs)
        ring = len(pixels(image, *near_player, FLASH, 10))
        if ring > best:
            best = ring
            blast_crops = [crop(image, top["x"] - 20 * cs, top["y"] - 70 * cs, 200 * cs, 130 * cs)]
    print(f"rocket blast: best fireball pixels {best}")
    check(best >= 300, f"the explosion should read ({best})")
    sheet(blast_crops, ROOT / "shots" / "jetpack-fire-rocket-blast.png", cols=1)
    assert not r.read_console(item), r.read_console(item)
    print("jetpack fire: flash at the muzzle per gun kind, tracers, casings, rocket and blast, mirrored facing, flash gone in time passed")
assert not FAILS or os.environ.get("JETPACK_FIRE_SOFT"), FAILS
