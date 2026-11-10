#!/usr/bin/env python3
"""C-1..C-3 of docs/design/2d-characters.md: `spine_to_lua.py` on the Paka fixture and on
hand-built two-bone rigs. The output is read back through a real Lua binary and played by a small
evaluator of the engine's own formulas (runtime/src/drawing.rs, world/src/clip.rs, plus the
`scale_x`/`scale_y`/`show`/`step`/bezier semantics the plan adds), then compared with the Spine
skeleton evaluated directly. Run: python3 scripts/test_spine_to_lua.py"""

import json
import math
import shutil
import struct
import subprocess
import sys
import tempfile
import unittest
import zlib
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import spine_to_lua as s2l  # noqa: E402

PAKA = HERE / "fixtures" / "paka"
PAKA_JSON = PAKA / "Character 1 - Paka.json"
PAKA_ATLAS = PAKA / "Character 1 - Paka1.atlas"
SPINE = HERE / "fixtures" / "spine"
LUA = next((shutil.which(n) for n in ("lua5.4", "lua", "luajit") if shutil.which(n)), None)

BAKED = {
    "leg_back", "hoof-back", "leg_front", "hoof_front",  # the two IK chains
    "head_control_back", "eyes_control", "eye_front", "eye_white_back",
    "head_nose", "head_ciolca", "head_ear_back", "head_ear_front",  # transform constraints
}
RIGID = {"tie", "palm_back", "palm_front", "head_ciolca", "head_ear_front", "head_ear_back", "head_smile"}
CHECKED = ("run", "attack", "jump", "idle")

# Dumps a Lua module's returned table as JSON; a table mixing list and named entries is an error.
DUMP = r"""
local function enc(v)
	local t = type(v)
	if t == "table" then
		local n, keys = #v, {}
		for k in pairs(v) do keys[#keys + 1] = k end
		local list = true
		for _, k in ipairs(keys) do
			if type(k) ~= "number" or k < 1 or k > n or k % 1 ~= 0 then list = false end
		end
		if list then
			local out = {}
			for i = 1, n do out[i] = enc(v[i]) end
			return "[" .. table.concat(out, ",") .. "]"
		end
		table.sort(keys, function(a, b) return tostring(a) < tostring(b) end)
		local out = {}
		for _, k in ipairs(keys) do
			if type(k) ~= "string" then error("table mixes list and named entries") end
			out[#out + 1] = string.format("%q", k) .. ":" .. enc(v[k])
		end
		return "{" .. table.concat(out, ",") .. "}"
	elseif t == "string" then return string.format("%q", v)
	elseif t == "number" then
		if v ~= v or v == math.huge or v == -math.huge then error("non-finite number") end
		return string.format("%.17g", v)
	elseif t == "boolean" then return tostring(v)
	end
	error("unexpected " .. t)
end
io.write(enc(dofile(arg[1])))
"""


def lua_load(path):
    if LUA is None:
        raise unittest.SkipTest("no lua binary on PATH")
    run = subprocess.run([LUA, "-e", DUMP.replace("arg[1]", repr(str(path))), ], capture_output=True, text=True)
    if run.returncode != 0:
        raise AssertionError(f"{path} does not load as Lua: {run.stderr}")
    return json.loads(run.stdout)


def convert(json_path, atlas, out, *extra):
    return subprocess.run(
        [sys.executable, "-I", str(HERE / "spine_to_lua.py"), str(json_path), str(atlas), str(out), *extra],
        capture_output=True, text=True,
    )


def png_size(path):
    with open(path, "rb") as f:
        head = f.read(24)
    return struct.unpack(">II", head[16:24])


def png_alpha(path):
    """Alpha rows of an 8-bit RGBA PNG, decoded with the stdlib."""
    data = Path(path).read_bytes()
    at, idat = 8, b""
    while at < len(data):
        n, kind = struct.unpack(">I4s", data[at:at + 8])
        body = data[at + 8:at + 8 + n]
        if kind == b"IHDR":
            w, h, depth, color, _, _, interlace = struct.unpack(">IIBBBBB", body)
            assert (depth, color, interlace) == (8, 6, 0), "only 8-bit RGBA, not interlaced"
        elif kind == b"IDAT":
            idat += body
        at += 12 + n
    raw, stride = zlib.decompress(idat), w * 4
    prev, rows = bytearray(stride), []
    for y in range(h):
        f, line = raw[y * (stride + 1)], bytearray(raw[y * (stride + 1) + 1:(y + 1) * (stride + 1)])
        for i in range(stride):
            a = line[i - 4] if i >= 4 else 0
            b, c = prev[i], (prev[i - 4] if i >= 4 else 0)
            if f == 1:
                line[i] = (line[i] + a) & 255
            elif f == 2:
                line[i] = (line[i] + b) & 255
            elif f == 3:
                line[i] = (line[i] + (a + b) // 2) & 255
            elif f == 4:
                q = a + b - c
                pa, pb, pc = abs(q - a), abs(q - b), abs(q - c)
                line[i] = (line[i] + (a if pa <= pb and pa <= pc else b if pb <= pc else c)) & 255
        rows.append(bytes(line[3::4]))
        prev = line
    return rows


# --- the engine, as the plan describes it ----------------------------------------------------

def mul(m, n):
    a, b, c, d, e, f = m
    a2, b2, c2, d2, e2, f2 = n
    return (a * a2 + c * b2, b * a2 + d * b2, a * c2 + c * d2, b * c2 + d * d2,
            a * e2 + c * f2 + e, b * e2 + d * f2 + f)


def apply(m, p):
    return (m[0] * p[0] + m[2] * p[1] + m[4], m[1] * p[0] + m[3] * p[1] + m[5])


def translate(x, y):
    return (1, 0, 0, 1, x, y)


def css_bezier(x1, y1, x2, y2, u):
    def coord(p1, p2, s):
        return 3 * (1 - s) ** 2 * s * p1 + 3 * (1 - s) * s * s * p2 + s ** 3
    lo, hi = 0.0, 1.0
    for _ in range(80):
        mid = (lo + hi) / 2
        lo, hi = (mid, hi) if coord(x1, x2, mid) < u else (lo, mid)
    return coord(y1, y2, (lo + hi) / 2)


def ease(easing, u):
    if easing is None or easing == "linear":
        return u
    if easing == "in_out":
        return u * u * (3 - 2 * u)
    if easing == "step":
        return 0.0
    return css_bezier(*easing, u)


def value_at(keys, t):
    n = sum(1 for k in keys if k[0] <= t)
    if n == 0:
        return keys[0][1]
    if n == len(keys):
        return keys[-1][1]
    a, b = keys[n - 1], keys[n]
    if isinstance(a[1], bool):
        return a[1]
    u = ease(b[2] if len(b) > 2 else None, (t - a[0]) / (b[0] - a[0]))
    return a[1] + (b[1] - a[1]) * u


def sample(clip, t):
    t = t % clip["length"] if clip.get("loop") else min(max(t, 0.0), clip["length"])
    return {part: {prop: value_at(keys, t) for prop, keys in props.items()}
            for part, props in clip["tracks"].items()}


def worlds(rig, poses):
    parts = {p["id"]: p for p in rig["parts"]}
    memo = {}

    def world(pid):
        if pid in memo:
            return memo[pid]
        part, pose = parts[pid], poses.get(pid, {})
        px, py = part["pivot"]
        r = math.radians(pose.get("rot", 0.0))
        s = pose.get("scale", 1.0)
        sx, sy = s * pose.get("scale_x", 1.0), s * pose.get("scale_y", 1.0)
        local = translate(pose.get("x", 0.0) + px, pose.get("y", 0.0) + py)
        local = mul(local, (math.cos(r) * sx, math.sin(r) * sx, -math.sin(r) * sy, math.cos(r) * sy, 0, 0))
        local = mul(local, translate(-px, -py))
        w = mul(world(part["parent"]), local) if part.get("parent") else local
        memo[pid] = w
        return w

    return {pid: world(pid) for pid in parts}


# --- strictness the engine applies (gfx.rs + the plan's new fields) ---------------------------

def check_drawing(test, rig, atlas_png):
    test.assertLessEqual(set(rig), {"size", "parts", "vars"})
    test.assertEqual(len(rig["size"]), 2)
    test.assertTrue(all(v > 0 for v in rig["size"]))
    w, h = png_size(atlas_png)
    ids = [p["id"] for p in rig["parts"]]
    test.assertEqual(len(ids), len(set(ids)), "duplicate part id")
    test.assertLessEqual(len(ids), 256, "MAX_PARTS")
    for part in rig["parts"]:
        test.assertLessEqual(set(part), {"id", "parent", "pivot", "shapes"})
        test.assertEqual(len(part["pivot"]), 2)
        if "parent" in part:
            test.assertIn(part["parent"], ids)
        for shape in part.get("shapes", []):
            test.assertEqual(set(shape), {"image", "src", "transform"})
            x, y, sw, sh = shape["src"]
            test.assertTrue(0 <= x and 0 <= y and sw > 0 and sh > 0 and x + sw <= w and y + sh <= h,
                            f"{part['id']} src {shape['src']} outside {w}x{h}")
            test.assertEqual(len(shape["transform"]), 6)
    return set(ids)


def check_clip(test, clip, part_ids, where):
    test.assertLessEqual(set(clip), {"length", "loop", "tracks"}, where)
    test.assertGreater(clip["length"], 0, where)
    test.assertIsInstance(clip.get("loop", False), bool, where)
    for part, props in clip["tracks"].items():
        test.assertIn(part, part_ids, f"{where}: unknown part {part}")
        test.assertLessEqual(set(props), {"x", "y", "rot", "scale", "scale_x", "scale_y", "show"}, where)
        for prop, keys in props.items():
            at = f"{where} {part}.{prop}"
            test.assertTrue(keys, at)
            times = [k[0] for k in keys]
            test.assertTrue(all(0 <= t <= clip["length"] for t in times), at)
            test.assertTrue(all(a < b for a, b in zip(times, times[1:])), f"{at}: times not rising")
            for k in keys:
                if prop == "show":
                    test.assertEqual(len(k), 2, f"{at}: a show key takes no easing")
                    test.assertIsInstance(k[1], bool, at)
                    continue
                test.assertIn(len(k), (2, 3), at)
                test.assertTrue(math.isfinite(k[1]), at)
                if len(k) == 3:
                    e = k[2]
                    if isinstance(e, list):
                        test.assertEqual(len(e), 4, at)
                        test.assertTrue(0 <= e[0] <= 1 and 0 <= e[2] <= 1, f"{at}: bezier x outside 0..1")
                    else:
                        test.assertIn(e, ("linear", "in_out", "step"), at)


# --- the comparison: Spine input against the converted output --------------------------------

def compare(model, out, report, anim, times):
    """Largest distance, in drawing units, between where Spine puts each bone (origin and two
    points on its axes) and each mesh vertex, and where the converted drawing + clip put them."""
    rig = lua_load(out / "rig.lua")
    entry = next(c for c in report["clips"] if c["name"] == anim)
    clip = lua_load(out / "clips" / f"{entry['file']}.lua")
    ox, oy = report["origin"]
    s = report["scale"]
    parts = {p["id"]: p for p in rig["parts"]}
    to_drawing = lambda p: (ox + s * p[0], oy - s * p[1])  # noqa: E731
    bone_err, vertex_err, show_bad = 0.0, {}, []
    for t in times:
        sp = model.pose(anim, t)
        w = worlds(rig, sample(clip, t))
        for i, name in enumerate(model.bone_names):
            a, b, c, d, wx, wy = sp[i]
            px, py = parts[name]["pivot"]
            for u in ((0, 0), (100, 0), (0, 100)):
                want = to_drawing((wx + a * u[0] + b * u[1], wy + c * u[0] + d * u[1]))
                got = apply(w[name], (px + s * u[0], py - s * u[1]))
                bone_err = max(bone_err, math.dist(want, got))
        visible = model.visible(anim, t)
        poses = sample(clip, t)
        for slot, pid in report["slot_parts"].items():
            if not parts[pid].get("shapes"):
                continue
            shown = poses.get(pid, {}).get("show", True)
            if shown != visible[slot]:
                show_bad.append((t, slot, visible[slot], shown))
            if not visible[slot] or not parts[pid].get("shapes"):
                continue
            shape = parts[pid]["shapes"][0]
            sx, sy = shape["src"][0], shape["src"][1]
            for page_px, world in model.vertices(slot, sp):
                want = to_drawing(world)
                got = apply(w[pid], apply(shape["transform"], (page_px[0] - sx, page_px[1] - sy)))
                vertex_err[slot] = max(vertex_err.get(slot, 0.0), math.dist(want, got))
    return bone_err, vertex_err, show_bad


class Paka(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp = Path(tempfile.mkdtemp(prefix="spine_to_lua_"))
        cls.out = cls.tmp / "paka"
        cls.conversion = convert(PAKA_JSON, PAKA_ATLAS, cls.out, "--image", "characters/paka/atlas.png")
        if cls.conversion.returncode != 0:
            raise AssertionError(f"converter failed: {cls.conversion.stderr}")
        cls.report = json.loads((cls.out / "report.json").read_text())
        cls.model = s2l.load(PAKA_JSON, PAKA_ATLAS, "classic")

    @classmethod
    def tearDownClass(cls):
        shutil.rmtree(cls.tmp)

    def test_c1_rig_and_clips_load_and_are_well_formed(self):
        rig = lua_load(self.out / "rig.lua")
        ids = check_drawing(self, rig, self.out / "atlas.png")
        data = json.loads(PAKA_JSON.read_text())
        bones, slots = data["bones"], data["slots"]
        self.assertEqual(len(bones), 58)
        self.assertEqual(len(slots), 31)
        bone_parts, slot_parts = rig["parts"][:58], rig["parts"][58:]
        self.assertEqual([p["id"] for p in bone_parts], [b["name"] for b in bones])
        for part, bone in zip(bone_parts, bones):
            self.assertNotIn("shapes", part)
            self.assertEqual(part.get("parent"), bone.get("parent"))
        self.assertEqual(len(slot_parts), 31)
        self.assertEqual([p["id"] for p in slot_parts], [self.report["slot_parts"][s["name"]] for s in slots])
        drawn = self.report["drawn_on"]
        for part, slot in zip(slot_parts, slots):
            if slot["name"] in self.report["empty_slots"]:
                self.assertNotIn("shapes", part)
                continue
            self.assertEqual(len(part["shapes"]), 1, slot["name"])
            self.assertEqual(part["shapes"][0]["image"], "characters/paka/atlas.png")
            self.assertEqual(part["parent"], drawn[slot["name"]])
        self.assertEqual(self.report["empty_slots"], ["path_death"])  # no attachment in any skin
        files = sorted((self.out / "clips").glob("*.lua"))
        self.assertEqual(len(files), 25)
        for f in files:
            check_clip(self, lua_load(f), ids, f.name)
        check_clip(self, lua_load(self.out / "setup.lua"), ids, "setup.lua")
        self.assertEqual(png_size(self.out / "atlas.png"), (1858, 910))

    def test_c2_report_names_baked_bones_and_rigid_meshes(self):
        self.assertEqual(set(self.report["baked"]), BAKED)
        rigid = {r["slot"]: r for r in self.report["rigid"]}
        self.assertEqual(set(rigid), RIGID)
        for r in rigid.values():
            self.assertIn(r["bone"], r["weights"])
            self.assertGreaterEqual(r["fit_error"], 0)
        # Rectangles bound to one bone are exact, not approximations.
        self.assertTrue(all(e <= 0.01 for e in self.report["exact_fit_error"].values()))

    def test_rotated_regions_land_on_their_art(self):
        """The one convention the Spine-vs-output comparison cannot see, since both sides use it:
        where a region stored turned 90 degrees puts its pixels. Each mesh's triangle centres must
        land on painted atlas pixels, and the opposite turn must land on fewer."""
        alpha = png_alpha(PAKA / "Character 1 - Paka1.png")
        data = json.loads(PAKA_JSON.read_text())
        skin = next(sk for sk in data["skins"] if sk["name"] == "classic")["attachments"]
        turned, used_total, other_total = 0, 0.0, 0.0
        for slot, pic in self.model.pictures.items():
            r, mesh = pic["region"], skin[slot][self.model.attachment[slot]]
            tri, uv = mesh["triangles"], mesh["uvs"]
            centres = [(sum(uv[2 * i] for i in tri[k:k + 3]) / 3 * r["w"], sum(uv[2 * i + 1] for i in tri[k:k + 3]) / 3 * r["h"])
                       for k in range(0, len(tri), 3)]
            painted = lambda X, Y: alpha[int(Y)][int(X)] > 0  # noqa: E731
            used = sum(painted(*s2l.page_px(r, px, py)) for px, py in centres) / len(centres)
            self.assertGreaterEqual(used, 0.75, slot)
            if r["rotate"]:
                turned += 1
                used_total += used
                other_total += sum(painted(r["x"] + r["h"] - py, r["y"] + px) for px, py in centres) / len(centres)
        self.assertEqual(turned, 4)  # classic's page; the other 8 rotated regions are dark/pop's
        self.assertGreater(used_total - other_total, 1.0)

    def test_output_agrees_with_spine(self):
        """On the 60 Hz grid the drawing + clip reproduce Spine but for rounding and the shear the
        transform constraints leave on baked bones (<0.04 degrees, which the engine cannot say).
        Half a sample off the grid the baked bones are interpolated linearly: a few units at most
        on a ~1200-unit character, under a pixel at the pack's 0.2 scale."""
        worst = {}
        for anim in CHECKED:
            entry = next(c for c in self.report["clips"] if c["name"] == anim)
            on = [round(i * entry["length"] / 8 * 60) / 60 for i in range(8)]
            off = [t + 1 / 120 for t in on]
            bone_on, verts_on, show_on = compare(self.model, self.out, self.report, anim, on)
            bone_off, verts_off, show_off = compare(self.model, self.out, self.report, anim, off)
            self.assertEqual(show_on + show_off, [], anim)
            exact_on = max(v for k, v in verts_on.items() if k not in RIGID)
            exact_off = max(v for k, v in verts_off.items() if k not in RIGID)
            rigid = max(max(verts_on.get(k, 0), verts_off.get(k, 0)) for k in RIGID)
            worst[anim] = (bone_on, exact_on, bone_off, exact_off, rigid)
            self.assertLess(bone_on, 0.1, f"{anim}: bones on the 60 Hz grid")
            self.assertLess(exact_on, 0.1, f"{anim}: one-bone meshes on the grid")
            self.assertLess(max(bone_off, exact_off), 5.0, f"{anim}: between baked samples")
        print("\n  max error in drawing units (Paka is ~1200 tall; at the pack's 0.2 scale a unit is 0.2 px)", file=sys.stderr)
        for anim, (a, b, c, d, e) in worst.items():
            print(f"  {anim:7} on-grid: bones {a:.1e}, one-bone meshes {b:.4f} | half a sample off: bones {c:.3f},"
                  f" one-bone meshes {d:.3f} | rigid-drawn skinned meshes {e:.1f}", file=sys.stderr)


class TwoBone(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="spine_to_lua_"))

    def tearDown(self):
        shutil.rmtree(self.tmp)

    def converted(self, name, *extra):
        out = self.tmp / name
        run = convert(SPINE / f"{name}.json", SPINE / "tiny.atlas", out, *extra)
        self.assertEqual(run.returncode, 0, run.stderr)
        return out, json.loads((out / "report.json").read_text())

    def test_c3_rotation_unwrapped_y_flipped_once_curve_moved(self):
        out, report = self.converted("fk")
        rig = lua_load(out / "rig.lua")
        ids = check_drawing(self, rig, out / "atlas.png")
        clip = lua_load(out / "clips" / "swing.lua")
        check_clip(self, clip, ids, "swing")
        pivots = {p["id"]: p["pivot"] for p in rig["parts"]}
        # y is down: a bone 50 up in Spine sits 50 above its parent here; +30° in Spine is -30°.
        self.assertEqual([pivots["upper"][0] - pivots["root"][0], pivots["upper"][1] - pivots["root"][1]], [0, -50])
        setup = lua_load(out / "setup.lua")["tracks"]
        self.assertEqual(setup["upper"]["rot"], [[0, -30]])
        self.assertEqual(setup["lower"]["rot"], [[0, -170]])
        self.assertEqual(setup["lower"]["scale_x"], [[0, 1.5]])
        tracks = clip["tracks"]
        # 170° to -170° is 20° the short way. Before its first key Spine holds the setup pose.
        self.assertEqual(tracks["upper"]["rot"], [[0, -30], [0.25, -200, "step"], [0.75, -220]])
        # The curve Spine keeps on the key that starts a segment lands on the key ending it.
        self.assertEqual(tracks["lower"]["rot"], [[0, -170], [0.5, -200, [0.25, 0.1, 0.75, 0.9]], [1, -140, "step"]])
        self.assertEqual(tracks["upper"]["x"], [[0, 10], [1, 0]])
        self.assertEqual(tracks["upper"]["y"], [[0, -20], [1, 0]])
        self.assertEqual(tracks["slot:lower"]["show"], [[0, True], [0.6, False], [0.8, True]])
        for props in tracks.values():
            keys = props.get("rot", [])
            self.assertTrue(all(abs(b[1] - a[1]) <= 180 for a, b in zip(keys, keys[1:])))
        model = s2l.load(SPINE / "fk.json", SPINE / "tiny.atlas", None)
        times = [i / 20 for i in range(21)]
        bone_err, vertex_err, show_bad = compare(model, out, report, "swing", times)
        self.assertLess(bone_err, 1e-3)
        self.assertLess(max(vertex_err.values()), 1e-3)
        self.assertEqual(show_bad, [])

    def test_c3_two_bone_ik_baked_to_the_analytic_answer(self):
        out, report = self.converted("ik")
        self.assertEqual(report["baked"], ["a", "b"])
        rig = lua_load(out / "rig.lua")
        clip = lua_load(out / "clips" / "reach.lua")
        check_clip(self, clip, check_drawing(self, rig, out / "atlas.png"), "reach")
        for t in (0, 0.25, 0.5, 0.75, 1):
            tx, ty = 100 - 100 * t, 100 + 100 * t  # the target, Spine y-up
            d2 = tx * tx + ty * ty
            a2 = math.acos((d2 - 2 * 100 * 100) / (2 * 100 * 100))  # bendPositive: +
            a1 = math.atan2(ty, tx) - math.atan2(100 * math.sin(a2), 100 + 100 * math.cos(a2))
            pose = sample(clip, t)
            self.assertAlmostEqual(pose["a"]["rot"], -math.degrees(a1), places=4)
            self.assertAlmostEqual(pose["b"]["rot"], -math.degrees(a2), places=4)
            parts = {p["id"]: p for p in rig["parts"]}
            w = worlds(rig, pose)
            bx, by = parts["b"]["pivot"]
            tip = apply(w["b"], (bx + 100, by))
            ox, oy = report["origin"]
            self.assertAlmostEqual(tip[0], ox + tx, places=3)
            self.assertAlmostEqual(tip[1], oy - ty, places=3)
        self.assertAlmostEqual(sample(clip, 0)["a"]["rot"], 0, places=6)
        self.assertAlmostEqual(sample(clip, 0)["b"]["rot"], -90, places=6)
        self.assertAlmostEqual(sample(clip, 1)["a"]["rot"], -90, places=4)

    def test_c2_unknown_features_are_errors(self):
        base = json.loads((SPINE / "fk.json").read_text())
        cases = {
            "deform": lambda d: d["animations"]["swing"].setdefault("deform", {"default": {}}),
            "drawOrder": lambda d: d["animations"]["swing"].setdefault("drawOrder", [{"time": 0}]),
            "events": lambda d: d.setdefault("events", {"hit": {}}),
            "path": lambda d: d.setdefault("path", [{"name": "p", "bones": ["upper"], "target": "upper"}]),
            "color": lambda d: d["animations"]["swing"]["slots"]["lower"].setdefault("color", [{"time": 0}]),
            "shear": lambda d: d["animations"]["swing"]["bones"]["upper"].setdefault("shear", [{"time": 0}]),
            "noScale": lambda d: d["bones"][1].setdefault("transform", "noScale"),
            "clipping": lambda d: d["skins"][0]["attachments"]["upper"].setdefault("clip", {"type": "clipping"}),
            "shearX": lambda d: d["bones"][2].setdefault("shearX", 5),
        }
        for feature, inject in cases.items():
            data = json.loads(json.dumps(base))
            inject(data)
            path = self.tmp / f"{feature}.json"
            path.write_text(json.dumps(data))
            run = convert(path, SPINE / "tiny.atlas", self.tmp / f"out_{feature}")
            self.assertNotEqual(run.returncode, 0, feature)
            self.assertIn(feature, run.stderr, feature)
            self.assertFalse((self.tmp / f"out_{feature}" / "rig.lua").exists(), feature)


if __name__ == "__main__":
    print(f"Lua used to parse the output: {LUA or 'none (tests that parse Lua are skipped)'}", file=sys.stderr)
    unittest.main()
