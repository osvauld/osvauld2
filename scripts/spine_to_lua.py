#!/usr/bin/env python3
"""Spine 3.8 JSON rig -> Osvauld drawing + clips (docs/design/2d-characters.md, step 4).

    python3 -I scripts/spine_to_lua.py RIG.json RIG.atlas OUT [--skin NAME] [--image PATH]
                                       [--scale S] [--loop NAME,...] [--fps 60]

Writes OUT/rig.lua (the table gfx.drawing takes), OUT/clips/<name>.lua (one gfx.clip table per
animation), OUT/setup.lua (the setup pose as a clip), OUT/atlas.png (the skin's atlas page) and
OUT/report.json. The input is data: anything this script does not understand is an error naming
it, never a guess.

Conventions of the output (the engine's: y down, rot in degrees, a pose is about the pivot):
- Rest pose: every bone part sits unrotated and unscaled at its setup offset from its parent.
  Rotation and scale live in the clips, as absolute local values; x/y are offsets from that rest,
  i.e. Spine's own translate values. Only this rest makes Spine's per-bone non-uniform scale exact,
  since the engine scales along the drawing's axes at rest. So every clip carries the setup value
  of every bone it does not animate, and setup.lua is the pose to show when nothing plays.
- Spine is y-up: y and rotation are negated once; scale is unchanged.
- A Spine curve sits on the key that starts a segment; the engine's easing on the key that ends
  it, so each curve moves one key along. Before its first key Spine holds the setup pose; the
  converter writes that as a key at 0 and steps to the first key.
- Bones under an IK or transform constraint are baked: the skeleton is evaluated with Spine 3.8's
  constraint math at --fps and their resulting local values are written as linear keys.
- Each slot is one part on the bone that carries its picture; a mesh skinned to several bones is
  drawn rigid on its heaviest bone (affine fit, reported).
"""

import argparse
import json
import math
import re
import shutil
import sys
from pathlib import Path


class ConvertError(Exception):
    pass


def fail(msg):
    raise ConvertError(msg)


DEG = math.pi / 180


def cosd(a):
    return math.cos(a * DEG)


def sind(a):
    return math.sin(a * DEG)


def wrap180(r):
    """Spine 3.8's own wrap: (-180, 180], 180 itself becomes -180 exactly as the runtime does."""
    return r - (16384 - int(16384.499999999996 - r / 360)) * 360


# --- input checking --------------------------------------------------------------------------

TOP = {"skeleton", "bones", "slots", "ik", "transform", "skins", "animations"}
TOP_UNSUPPORTED = {"path": "path constraints", "events": "events"}
BONE_FIELDS = {"name", "parent", "length", "x", "y", "rotation", "scaleX", "scaleY", "shearX", "shearY",
               "transform", "skin", "color"}
SLOT_FIELDS = {"name", "bone", "attachment", "color", "dark", "blend"}
IK_FIELDS = {"name", "order", "skin", "bones", "target", "mix", "softness", "bendPositive", "compress",
             "stretch", "uniform"}
TC_FIELDS = {"name", "order", "skin", "bones", "target", "rotation", "x", "y", "scaleX", "scaleY", "shearY",
             "rotateMix", "translateMix", "scaleMix", "shearMix", "local", "relative"}
ATTACHMENT_FIELDS = {
    "region": {"type", "name", "path", "x", "y", "scaleX", "scaleY", "rotation", "width", "height", "color"},
    "mesh": {"type", "name", "path", "uvs", "triangles", "vertices", "hull", "edges", "width", "height", "color"},
    "linkedmesh": {"type", "name", "path", "skin", "parent", "deform", "width", "height", "color"},
}
KEY_FIELDS = {
    "rotate": {"time", "angle", "curve", "c2", "c3", "c4"},
    "translate": {"time", "x", "y", "curve", "c2", "c3", "c4"},
    "scale": {"time", "x", "y", "curve", "c2", "c3", "c4"},
    "attachment": {"time", "name"},
}
WHITE = (None, "ffffffff", "FFFFFFFF")


def unknown(fields, allowed, what):
    extra = sorted(set(fields) - allowed)
    if extra:
        fail(f"{what}: unknown field {extra[0]!r}")


def check(d):
    for key, value in d.items():
        if key in TOP_UNSUPPORTED:
            if value:
                fail(f"top-level {key!r}: {TOP_UNSUPPORTED[key]} are not supported")
        elif key not in TOP:
            fail(f"top-level {key!r} is not a Spine 3.8 section this converter knows")
    version = d.get("skeleton", {}).get("spine", "")
    if not version.startswith("3.8"):
        fail(f"skeleton.spine is {version!r}; only Spine 3.8 JSON is supported")
    for b in d["bones"]:
        what = f"bone {b.get('name')!r}"
        unknown(b, BONE_FIELDS, what)
        for shear in ("shearX", "shearY"):
            if b.get(shear, 0):
                fail(f"{what}: {shear} is not supported (the engine has no shear)")
        if b.get("transform", "normal") != "normal":
            fail(f"{what}: transform inherit mode {b['transform']!r} is not supported (only 'normal')")
        if b.get("skin"):
            fail(f"{what}: skin-required bones are not supported")
    for s in d["slots"]:
        what = f"slot {s.get('name')!r}"
        unknown(s, SLOT_FIELDS, what)
        if s.get("color") not in WHITE:
            fail(f"{what}: slot color {s['color']} is not supported")
        if "dark" in s:
            fail(f"{what}: two-colour tinting (dark) is not supported")
        if s.get("blend", "normal") != "normal":
            fail(f"{what}: blend mode {s['blend']!r} is not supported")
    for c in d.get("ik", []):
        unknown(c, IK_FIELDS, f"ik constraint {c.get('name')!r}")
        if c.get("skin"):
            fail(f"ik constraint {c['name']!r}: skin-required constraints are not supported")
    for c in d.get("transform", []):
        unknown(c, TC_FIELDS, f"transform constraint {c.get('name')!r}")
        for mode in ("local", "relative", "skin"):
            if c.get(mode):
                fail(f"transform constraint {c['name']!r}: {mode} mode is not supported")
    for skin in d["skins"]:
        for extra in ("bones", "ik", "transform", "path"):
            if skin.get(extra):
                fail(f"skin {skin['name']!r}: skin {extra} are not supported")
        for slot, atts in skin.get("attachments", {}).items():
            for name, a in atts.items():
                kind = a.get("type", "region")
                what = f"skin {skin['name']!r} attachment {slot}/{name}"
                if kind not in ATTACHMENT_FIELDS:
                    fail(f"{what}: attachment type {kind!r} is not supported")
                unknown(a, ATTACHMENT_FIELDS[kind], what)
                if a.get("color") not in WHITE:
                    fail(f"{what}: attachment color {a['color']} is not supported")
    for name, anim in d["animations"].items():
        for section, timelines in anim.items():
            if section not in ("bones", "slots"):
                fail(f"animation {name!r}: {section} timelines are not supported")
            for owner, tls in timelines.items():
                for kind, keys in tls.items():
                    if kind not in KEY_FIELDS or (section == "bones") != (kind != "attachment"):
                        fail(f"animation {name!r}: {section[:-1]} {owner!r} has {kind} keys, which are not supported")
                    for k in keys:
                        unknown(k, KEY_FIELDS[kind], f"animation {name!r} {owner}.{kind} key")


# --- atlas -----------------------------------------------------------------------------------

def read_atlas(path):
    pages, regions, page, region = [], {}, None, None
    for raw in Path(path).read_text().splitlines():
        line = raw.strip()
        if not line:
            page = region = None
            continue
        if ":" not in line:
            if page is None:
                page = {"file": line}
                pages.append(page)
            else:
                region = {"page": page["file"]}
                regions[line] = region
            continue
        key, value = (s.strip() for s in line.split(":", 1))
        (region if region is not None else page)[key] = value
    out = {}
    for name, r in regions.items():
        what = f"atlas region {name!r}"
        if r.get("rotate", "false") not in ("true", "false"):
            fail(f"{what}: rotate {r['rotate']!r} is not supported (true or false)")
        x, y = (int(v) for v in r["xy"].split(","))
        w, h = (int(v) for v in r["size"].split(","))
        if r.get("orig", r["size"]).replace(" ", "") != r["size"].replace(" ", "") or \
                r.get("offset", "0,0").replace(" ", "") != "0,0":
            fail(f"{what}: whitespace-stripped regions (orig/offset) are not supported")
        if r.get("index", "-1") != "-1":
            fail(f"{what}: indexed (sequence) regions are not supported")
        out[name] = {"page": r["page"], "x": x, "y": y, "w": w, "h": h, "rotate": r.get("rotate") == "true"}
    sizes = {p["file"]: tuple(int(v) for v in p["size"].split(",")) for p in pages}
    return out, sizes


def page_px(region, px, py):
    """Where pixel (px, py) of a region's upright image (y down) lies on its atlas page. A region
    with `rotate: true` is stored turned so the image's top edge runs up its left side: Spine 3.8's
    MeshAttachment.updateUVs, checked against a mesh hull on the Paka atlas."""
    if region["rotate"]:
        return (region["x"] + py, region["y"] + region["w"] - px)
    return (region["x"] + px, region["y"] + py)


# --- curves and timelines --------------------------------------------------------------------

def bezier_at(c, u, approx):
    cx1, cy1, cx2, cy2 = c
    if approx:  # Spine 3.8's runtime: the curve as 10 straight segments
        pts = [(0.0, 0.0)] + [(_cubic(cx1, cx2, s / 10), _cubic(cy1, cy2, s / 10)) for s in range(1, 10)] + [(1.0, 1.0)]
        for (x0, y0), (x1, y1) in zip(pts, pts[1:]):
            if x1 >= u:
                return y0 + (y1 - y0) * (u - x0) / (x1 - x0) if x1 > x0 else y1
        return 1.0
    lo, hi, s = 0.0, 1.0, u
    for _ in range(60):
        x = _cubic(cx1, cx2, s) - u
        if abs(x) < 1e-13:
            break
        lo, hi = (s, hi) if x < 0 else (lo, s)
        dx = 3 * (1 - s) ** 2 * cx1 + 6 * (1 - s) * s * (cx2 - cx1) + 3 * s * s * (1 - cx2)
        s = s - x / dx if dx > 1e-9 and lo < s - x / dx < hi else (lo + hi) / 2
    return _cubic(cy1, cy2, s)


def _cubic(p1, p2, s):
    return 3 * (1 - s) ** 2 * s * p1 + 3 * (1 - s) * s * s * p2 + s ** 3


def read_keys(keys, kind, what):
    out = []
    for k in keys:
        if kind == "rotate":
            value = (k.get("angle", 0.0),)
        else:
            default = 1.0 if kind == "scale" else 0.0
            value = (k.get("x", default), k.get("y", default))
        curve = k.get("curve")
        if isinstance(curve, (int, float)):
            curve = (float(curve), k.get("c2", 0.0), k.get("c3", 1.0), k.get("c4", 1.0))
            if not (0 <= curve[0] <= 1 and 0 <= curve[2] <= 1):
                fail(f"{what}: bezier x outside 0..1 at time {k.get('time', 0)}")
        elif curve not in (None, "stepped"):
            fail(f"{what}: unknown curve {curve!r}")
        out.append((float(k.get("time", 0.0)), value, curve))
    if any(b[0] <= a[0] for a, b in zip(out, out[1:])):
        fail(f"{what}: key times do not rise")
    return out


def timeline_at(keys, t, rotate, approx):
    """A Spine 3.8 timeline's value at t with alpha 1 and setup blend; None means the setup pose."""
    if t < keys[0][0]:
        return None
    if t >= keys[-1][0]:
        return keys[-1][1]
    i = max(j for j, k in enumerate(keys) if k[0] <= t)
    (t0, v0, curve), (t1, v1, _) = keys[i], keys[i + 1]
    u = (t - t0) / (t1 - t0)
    p = 0.0 if curve == "stepped" else u if curve is None else bezier_at(curve, u, approx)
    if rotate:
        return (v0[0] + wrap180(v1[0] - v0[0]) * p,)
    return tuple(a + (b - a) * p for a, b in zip(v0, v1))


def easing(curve):
    if curve is None:
        return None
    if curve == "stepped":
        return "step"
    return list(curve)


# --- the skeleton ----------------------------------------------------------------------------

class Model:
    """A Spine 3.8 skeleton with one skin, evaluated the way spine-libgdx 3.8 does."""

    def __init__(self, d, atlas, skin):
        check(d)
        self.data = d
        self.bone_names = [b["name"] for b in d["bones"]]
        index = {n: i for i, n in enumerate(self.bone_names)}
        if len(index) != len(self.bone_names):
            fail("duplicate bone name")
        self.bone_index = index
        self.parent, self.setup, self.length = [], [], []
        for b in d["bones"]:
            p = b.get("parent")
            if p is not None and (p not in index or index[p] >= index[b["name"]]):
                fail(f"bone {b['name']!r}: parent {p!r} is unknown or listed after it")
            if p is None and self.parent:
                fail(f"bone {b['name']!r}: only the first bone may be a root")
            self.parent.append(-1 if p is None else index[p])
            self.setup.append((b.get("x", 0.0), b.get("y", 0.0), b.get("rotation", 0.0),
                               b.get("scaleX", 1.0), b.get("scaleY", 1.0)))
            self.length.append(b.get("length", 0.0))
        self.children = [[j for j, p in enumerate(self.parent) if p == i] for i in range(len(self.parent))]

        self.slots = d["slots"]
        self.slot_index = {s["name"]: i for i, s in enumerate(self.slots)}
        for s in self.slots:
            if s["bone"] not in index:
                fail(f"slot {s['name']!r}: unknown bone {s['bone']!r}")

        def bones_of(c, kind):
            for n in c["bones"] + [c["target"]]:
                if n not in index:
                    fail(f"{kind} constraint {c['name']!r}: unknown bone {n!r}")
            return [index[n] for n in c["bones"]], index[c["target"]]

        self.constraints = []
        for c in d.get("ik", []):
            bones, target = bones_of(c, "ik")
            if len(bones) not in (1, 2) or self.parent[bones[0]] < 0:
                fail(f"ik constraint {c['name']!r}: needs one or two non-root bones")
            if len(bones) == 2 and self.parent[bones[1]] != bones[0]:
                fail(f"ik constraint {c['name']!r}: the second bone must be the first's child")
            self.constraints.append({
                "kind": "ik", "name": c["name"], "order": c.get("order", 0), "bones": bones, "target": target,
                "mix": c.get("mix", 1.0), "softness": c.get("softness", 0.0),
                "bend": 1 if c.get("bendPositive", True) else -1, "compress": c.get("compress", False),
                "stretch": c.get("stretch", False), "uniform": c.get("uniform", False)})
        for c in d.get("transform", []):
            bones, target = bones_of(c, "transform")
            self.constraints.append({
                "kind": "transform", "name": c["name"], "order": c.get("order", 0), "bones": bones,
                "target": target, "rotation": c.get("rotation", 0.0), "x": c.get("x", 0.0), "y": c.get("y", 0.0),
                "scaleX": c.get("scaleX", 0.0), "scaleY": c.get("scaleY", 0.0), "shearY": c.get("shearY", 0.0),
                "rotateMix": c.get("rotateMix", 1.0), "translateMix": c.get("translateMix", 1.0),
                "scaleMix": c.get("scaleMix", 1.0), "shearMix": c.get("shearMix", 1.0)})
        orders = [c["order"] for c in self.constraints]
        if len(set(orders)) != len(orders):
            fail("two constraints share an order; Spine would skip one")
        self._build_cache()

        self._read_skin(d, atlas, skin)
        self._read_animations(d)

    # Skeleton.updateCache: constraints in `order`, each after the bones it reads, the bones it
    # moves re-sorted after it.
    def _build_cache(self):
        n = len(self.parent)
        sorted_ = [False] * n
        cache, reset = [], []

        def sort_bone(i):
            if sorted_[i]:
                return
            if self.parent[i] >= 0:
                sort_bone(self.parent[i])
            sorted_[i] = True
            cache.append(("bone", i))

        def sort_reset(bones):
            for i in bones:
                if sorted_[i]:
                    sort_reset(self.children[i])
                sorted_[i] = False

        for c in sorted(self.constraints, key=lambda c: c["order"]):
            sort_bone(c["target"])
            if c["kind"] == "ik":
                parent = c["bones"][0]
                sort_bone(parent)
                if len(c["bones"]) > 1:
                    child = c["bones"][-1]
                    if ("bone", child) not in cache:
                        reset.append(child)
                cache.append(("constraint", c))
                sort_reset(self.children[parent])
                sorted_[c["bones"][-1]] = True
            else:
                for b in c["bones"]:
                    sort_bone(b)
                cache.append(("constraint", c))
                for b in c["bones"]:
                    sort_reset(self.children[b])
                for b in c["bones"]:
                    sorted_[b] = True
        for i in range(n):
            sort_bone(i)
        self.cache, self.reset = cache, reset

    def _read_skin(self, d, atlas, skin):
        regions, page_sizes = atlas
        skins = {s["name"]: s.get("attachments", {}) for s in d["skins"]}
        if skin is None:
            skin = next((n for n in skins if n != "default"), "default")
        if skin not in skins:
            fail(f"no skin {skin!r}; the file has {sorted(skins)}")
        self.skin = skin
        chosen = {}
        for name in ("default", skin):
            for slot, atts in skins.get(name, {}).items():
                if slot not in self.slot_index:
                    fail(f"skin {name!r}: unknown slot {slot!r}")
                chosen.setdefault(slot, {}).update(atts)

        def geometry(slot, name, a):
            if a.get("type", "region") == "linkedmesh":
                parent_skin = skins.get(a.get("skin", "default"), {})
                parent = parent_skin.get(slot, {}).get(a["parent"])
                if parent is None or parent.get("type") != "mesh":
                    fail(f"linked mesh {slot}/{name}: parent mesh {a['parent']!r} not found")
                return parent
            return a

        # The one attachment each slot ever shows: setup plus every attachment key.
        used = {s["name"]: set() for s in self.slots}
        for s in self.slots:
            if s.get("attachment") is not None:
                used[s["name"]].add(s["attachment"])
        for anim in d["animations"].values():
            for slot, tls in anim.get("slots", {}).items():
                if slot not in used:
                    fail(f"animation keys unknown slot {slot!r}")
                used[slot].update(k["name"] for k in tls.get("attachment", []) if k.get("name") is not None)

        self.attachment, self.pictures, pages = {}, {}, set()
        for s in self.slots:
            names = sorted(n for n in used[s["name"]] if n in chosen.get(s["name"], {}))
            if len(names) > 1:
                fail(f"slot {s['name']!r} switches between attachments {names}; only show/hide is supported")
            if not names:
                continue
            name = names[0]
            a = chosen[s["name"]][name]
            path = a.get("path", a.get("name", name))
            if path not in regions:
                fail(f"attachment {s['name']}/{name}: atlas has no region {path!r}")
            region = regions[path]
            pages.add(region["page"])
            self.attachment[s["name"]] = name
            self.pictures[s["name"]] = self._vertices(s, name, geometry(s["name"], name, a), region)
        if len(pages) > 1:
            fail(f"skin {skin!r} uses {len(pages)} atlas pages; one page per skin is supported")
        self.page = pages.pop() if pages else None
        self.page_size = page_sizes.get(self.page)

    def _vertices(self, slot, name, a, region):
        """(page pixel, binding) per vertex; binding is [(bone, x, y, weight)] in Spine units."""
        bone = self.bone_index[slot["bone"]]
        w, h = region["w"], region["h"]
        if a.get("type", "region") == "region":
            aw, ah = a["width"], a["height"]
            sx, sy, rot = a.get("scaleX", 1.0), a.get("scaleY", 1.0), a.get("rotation", 0.0)
            out = []
            for px, py in ((0, 0), (w, 0), (0, h), (w, h)):
                lx, ly = (px / w - 0.5) * aw * sx, (0.5 - py / h) * ah * sy
                x = a.get("x", 0.0) + lx * cosd(rot) - ly * sind(rot)
                y = a.get("y", 0.0) + lx * sind(rot) + ly * cosd(rot)
                out.append((page_px(region, px, py), [(bone, x, y, 1.0)]))
            return {"region": region, "kind": "region", "verts": out}
        uvs, verts = a["uvs"], a["vertices"]
        n = len(uvs) // 2
        pages = [page_px(region, uvs[2 * i] * w, uvs[2 * i + 1] * h) for i in range(n)]
        if len(verts) == len(uvs):
            binding = [[(bone, verts[2 * i], verts[2 * i + 1], 1.0)] for i in range(n)]
        else:
            binding, at = [], 0
            for _ in range(n):
                count = int(verts[at])
                at += 1
                binding.append([(int(verts[at + 4 * j]), verts[at + 4 * j + 1], verts[at + 4 * j + 2], verts[at + 4 * j + 3])
                                for j in range(count)])
                at += 4 * count
            if at != len(verts):
                fail(f"mesh {slot['name']}/{name}: vertex weights do not match its uvs")
        return {"region": region, "kind": "mesh", "verts": list(zip(pages, binding))}

    def _read_animations(self, d):
        self.anims = {}
        for name, anim in d["animations"].items():
            bones, slots, times = {}, {}, [0.0]
            for bone, tls in anim.get("bones", {}).items():
                if bone not in self.bone_index:
                    fail(f"animation {name!r}: unknown bone {bone!r}")
                bones[self.bone_index[bone]] = {
                    kind: read_keys(keys, kind, f"animation {name!r} {bone}.{kind}") for kind, keys in tls.items() if keys}
            for slot, tls in anim.get("slots", {}).items():
                keys = [(float(k.get("time", 0.0)), k.get("name")) for k in tls.get("attachment", [])]
                if any(b[0] <= a[0] for a, b in zip(keys, keys[1:])):
                    fail(f"animation {name!r}: slot {slot!r} attachment key times do not rise")
                if keys:
                    slots[slot] = keys
            for tls in bones.values():
                times += [k[0] for keys in tls.values() for k in keys]
            times += [k[0] for keys in slots.values() for k in keys]
            self.anims[name] = {"bones": bones, "slots": slots, "duration": max(times)}

    # --- evaluation ---

    def pose(self, anim, t, approx=False):
        """World matrices (a, b, c, d, x, y), Spine y-up, of every bone at time t of `anim`
        (None: the setup pose). `approx` uses Spine's 10-segment curves instead of exact beziers."""
        local = [list(s) for s in self.setup]
        if anim is not None:
            for i, tls in self.anims[anim]["bones"].items():
                x0, y0, r0, sx0, sy0 = self.setup[i]
                if "rotate" in tls:
                    v = timeline_at(tls["rotate"], t, True, approx)
                    if v is not None:
                        local[i][2] = r0 + wrap180(v[0])
                if "translate" in tls:
                    v = timeline_at(tls["translate"], t, False, approx)
                    if v is not None:
                        local[i][0], local[i][1] = x0 + v[0], y0 + v[1]
                if "scale" in tls:
                    v = timeline_at(tls["scale"], t, False, approx)
                    if v is not None:
                        local[i][3], local[i][4] = sx0 * v[0], sy0 * v[1]
        return _Solve(self, local).run()

    def visible(self, anim, t):
        out = {}
        for s in self.slots:
            name = s.get("attachment")
            keys = self.anims[anim]["slots"].get(s["name"]) if anim is not None else None
            if keys and t >= keys[0][0]:
                name = max((k for k in keys if k[0] <= t), key=lambda k: k[0])[1]
            out[s["name"]] = name is not None and name == self.attachment.get(s["name"])
        return out

    def vertices(self, slot, worlds):
        """(page pixel, Spine world position) of each vertex of a slot's picture, skinned exactly."""
        out = []
        for page, binding in self.pictures[slot]["verts"]:
            x = y = 0.0
            for bone, vx, vy, wt in binding:
                a, b, c, d, wx, wy = worlds[bone]
                x += (a * vx + b * vy + wx) * wt
                y += (c * vx + d * vy + wy) * wt
            out.append((page, (x, y)))
        return out


class _Solve:
    """One Skeleton.updateWorldTransform: the update cache over bones and constraints."""

    def __init__(self, model, local):
        self.m, self.local = model, local
        n = len(local)
        self.ap = [None] * n
        self.w = [None] * n
        self.valid = [False] * n

    def run(self):
        for i in self.m.reset:
            x, y, r, sx, sy = self.local[i]
            self.ap[i] = [x, y, r, sx, sy, 0.0, 0.0]
            self.valid[i] = True
        for kind, item in self.m.cache:
            if kind == "bone":
                self.update(item, *self.local[item], 0.0, 0.0)
            elif item["kind"] == "ik":
                tgt = self.w[item["target"]]
                if len(item["bones"]) == 1:
                    self.ik1(item["bones"][0], tgt[4], tgt[5], item["compress"], item["stretch"],
                             item["uniform"], item["mix"])
                else:
                    self.ik2(item["bones"][0], item["bones"][1], tgt[4], tgt[5], item["bend"],
                             item["stretch"], item["softness"], item["mix"])
            else:
                self.transform(item)
        return [tuple(w) for w in self.w]

    def update(self, i, x, y, rot, sx, sy, shx, shy):
        self.ap[i] = [x, y, rot, sx, sy, shx, shy]
        self.valid[i] = True
        p = self.m.parent[i]
        ry = rot + 90 + shy
        la, lb, lc, ld = cosd(rot + shx) * sx, cosd(ry) * sy, sind(rot + shx) * sx, sind(ry) * sy
        if p < 0:
            self.w[i] = [la, lb, lc, ld, x, y]
            return
        pa, pb, pc, pd, px, py = self.w[p]
        self.w[i] = [pa * la + pb * lc, pa * lb + pb * ld, pc * la + pd * lc, pc * lb + pd * ld,
                     pa * x + pb * y + px, pc * x + pd * y + py]

    def update_applied(self, i):
        self.valid[i] = True
        a, b, c, d, wx, wy = self.w[i]
        p = self.m.parent[i]
        self.ap[i] = list(applied(self.w[p] if p >= 0 else None, (a, b, c, d, wx, wy)))

    def ik1(self, i, tx, ty, compress, stretch, uniform, alpha):
        if not self.valid[i]:
            self.update_applied(i)
        ax, ay, arot, asx, asy, ashx, ashy = self.ap[i]
        pa, pb, pc, pd, pwx, pwy = self.w[self.m.parent[i]]
        rotation_ik = -ashx - arot
        x, y = tx - pwx, ty - pwy
        det = pa * pd - pb * pc
        tx, ty = (x * pd - y * pb) / det - ax, (y * pa - x * pc) / det - ay
        rotation_ik += math.atan2(ty, tx) / DEG
        if asx < 0:
            rotation_ik += 180
        if rotation_ik > 180:
            rotation_ik -= 360
        elif rotation_ik < -180:
            rotation_ik += 360
        sx, sy = asx, asy
        if compress or stretch:
            b = self.m.length[i] * sx
            dd = math.sqrt(tx * tx + ty * ty)
            if (compress and dd < b) or (stretch and dd > b and b > 0.0001):
                s = (dd / b - 1) * alpha + 1
                sx *= s
                if uniform:
                    sy *= s
        self.update(i, ax, ay, arot + rotation_ik * alpha, sx, sy, ashx, ashy)

    def ik2(self, parent, child, tx, ty, bend, stretch, softness, alpha):
        if alpha == 0:
            self.update(child, *self.local[child], 0.0, 0.0)
            return
        if not self.valid[parent]:
            self.update_applied(parent)
        if not self.valid[child]:
            self.update_applied(child)
        px, py, prot, psx, psy = self.ap[parent][:5]
        sx = psx
        cx, cy0, crot, csx, csy, cshx, cshy = self.ap[child]
        if psx < 0:
            psx, os1, s2 = -psx, 180, -1
        else:
            os1, s2 = 0, 1
        if psy < 0:
            psy, s2 = -psy, -s2
        csx_signed = csx
        if csx < 0:
            csx, os2 = -csx, 180
        else:
            os2 = 0
        a, b, c, d, pwx, pwy = self.w[parent]
        u = abs(psx - psy) <= 0.0001
        if not u:
            cy = 0.0
            cwx, cwy = a * cx + pwx, c * cx + pwy
        else:
            cy = cy0
            cwx, cwy = a * cx + b * cy + pwx, c * cx + d * cy + pwy
        a, b, c, d, ppx, ppy = self.w[self.m.parent[parent]]
        inv = 1 / (a * d - b * c)
        x, y = cwx - ppx, cwy - ppy
        dx, dy = (x * d - y * b) * inv - px, (y * a - x * c) * inv - py
        l1, l2 = math.sqrt(dx * dx + dy * dy), self.m.length[child] * csx
        if l1 < 0.0001:
            self.ik1(parent, tx, ty, False, stretch, False, alpha)
            self.update(child, cx, cy, 0.0, csx_signed, csy, cshx, cshy)
            return
        x, y = tx - ppx, ty - ppy
        tx, ty = (x * d - y * b) * inv - px, (y * a - x * c) * inv - py
        dd = tx * tx + ty * ty
        if softness != 0:
            softness *= psx * (csx + 1) / 2
            td = math.sqrt(dd)
            sd = td - l1 - l2 * psx + softness
            if sd > 0:
                p = min(1, sd / (softness * 2)) - 1
                p = (sd - softness * (1 - p * p)) / td
                tx -= p * tx
                ty -= p * ty
                dd = tx * tx + ty * ty
        if u:
            l2 *= psx
            cos = (dd - l1 * l1 - l2 * l2) / (2 * l1 * l2)
            if cos < -1:
                cos = -1
            elif cos > 1:
                cos = 1
                if stretch:
                    sx *= (math.sqrt(dd) / (l1 + l2) - 1) * alpha + 1
            a2 = math.acos(cos) * bend
            a = l1 + l2 * cos
            b = l2 * math.sin(a2)
            a1 = math.atan2(ty * a - tx * b, tx * a + ty * b)
        else:
            a1 = a2 = None
            a, b = psx * l2, psy * l2
            aa, bb, ta = a * a, b * b, math.atan2(ty, tx)
            c = bb * l1 * l1 + aa * dd - aa * bb
            c1, c2 = -2 * bb * l1, bb - aa
            d = c1 * c1 - 4 * c2 * c
            if d >= 0:
                q = math.sqrt(d)
                if c1 < 0:
                    q = -q
                q = -(c1 + q) / 2
                r0, r1 = q / c2, c / q
                r = r0 if abs(r0) < abs(r1) else r1
                if r * r <= dd:
                    y = math.sqrt(dd - r * r) * bend
                    a1 = ta - math.atan2(y, r)
                    a2 = math.atan2(y / psy, (r - l1) / psx)
            if a1 is None:
                min_angle, min_x, min_y = math.pi, l1 - a, 0.0
                min_dist = min_x * min_x
                max_angle, max_x, max_y = 0.0, l1 + a, 0.0
                max_dist = max_x * max_x
                c = -a * l1 / (aa - bb)
                if -1 <= c <= 1:
                    c = math.acos(c)
                    x, y = a * math.cos(c) + l1, b * math.sin(c)
                    d = x * x + y * y
                    if d < min_dist:
                        min_angle, min_dist, min_x, min_y = c, d, x, y
                    if d > max_dist:
                        max_angle, max_dist, max_x, max_y = c, d, x, y
                if dd <= (min_dist + max_dist) / 2:
                    a1 = ta - math.atan2(min_y * bend, min_x)
                    a2 = min_angle * bend
                else:
                    a1 = ta - math.atan2(max_y * bend, max_x)
                    a2 = max_angle * bend
        os = math.atan2(cy, cx) * s2
        a1 = (a1 - os) / DEG + os1 - prot
        if a1 > 180:
            a1 -= 360
        elif a1 < -180:
            a1 += 360
        self.update(parent, px, py, prot + a1 * alpha, sx, self.ap[parent][4], 0.0, 0.0)
        a2 = ((a2 + os) / DEG - cshx) * s2 + os2 - crot
        if a2 > 180:
            a2 -= 360
        elif a2 < -180:
            a2 += 360
        self.update(child, cx, cy, crot + a2 * alpha, csx_signed, csy, cshx, cshy)

    def transform(self, c):
        """TransformConstraint.applyAbsoluteWorld."""
        ta, tb, tc, td, twx, twy = self.w[c["target"]]
        reflect = DEG if ta * td - tb * tc > 0 else -DEG
        off_rot, off_shear = c["rotation"] * reflect, c["shearY"] * reflect
        for i in c["bones"]:
            w = self.w[i]
            modified = False
            if c["rotateMix"] != 0:
                a, b, cc, d = w[:4]
                r = math.atan2(tc, ta) - math.atan2(cc, a) + off_rot
                if r > math.pi:
                    r -= 2 * math.pi
                elif r < -math.pi:
                    r += 2 * math.pi
                r *= c["rotateMix"]
                cos, sin = math.cos(r), math.sin(r)
                w[0], w[1], w[2], w[3] = cos * a - sin * cc, cos * b - sin * d, sin * a + cos * cc, sin * b + cos * d
                modified = True
            if c["translateMix"] != 0:
                ox, oy = ta * c["x"] + tb * c["y"] + twx, tc * c["x"] + td * c["y"] + twy
                w[4] += (ox - w[4]) * c["translateMix"]
                w[5] += (oy - w[5]) * c["translateMix"]
                modified = True
            if c["scaleMix"] > 0:
                s, ts = math.sqrt(w[0] ** 2 + w[2] ** 2), math.sqrt(ta * ta + tc * tc)
                if s > 0.00001:
                    s = (s + (ts - s + c["scaleX"]) * c["scaleMix"]) / s
                w[0] *= s
                w[2] *= s
                s, ts = math.sqrt(w[1] ** 2 + w[3] ** 2), math.sqrt(tb * tb + td * td)
                if s > 0.00001:
                    s = (s + (ts - s + c["scaleY"]) * c["scaleMix"]) / s
                w[1] *= s
                w[3] *= s
                modified = True
            if c["shearMix"] > 0:
                b, d = w[1], w[3]
                by = math.atan2(d, b)
                r = math.atan2(td, tb) - math.atan2(tc, ta) - (by - math.atan2(w[2], w[0]))
                if r > math.pi:
                    r -= 2 * math.pi
                elif r < -math.pi:
                    r += 2 * math.pi
                r = by + (r + off_shear) * c["shearMix"]
                s = math.sqrt(b * b + d * d)
                w[1], w[3] = math.cos(r) * s, math.sin(r) * s
                modified = True
            if modified:
                self.valid[i] = False


def applied(parent, world):
    """Bone.updateAppliedTransform: the local (x, y, rot, sx, sy, shearX=0, shearY) giving `world`."""
    a, b, c, d, wx, wy = world
    if parent is None:
        return (wx, wy, math.atan2(c, a) / DEG, math.sqrt(a * a + c * c), math.sqrt(b * b + d * d), 0.0,
                math.atan2(a * b + c * d, a * d - b * c) / DEG)
    pa, pb, pc, pd, px, py = parent
    pid = 1 / (pa * pd - pb * pc)
    dx, dy = wx - px, wy - py
    ax, ay = dx * pd * pid - dy * pb * pid, dy * pa * pid - dx * pc * pid
    ia, idd, ib, ic = pid * pd, pid * pa, pid * pb, pid * pc
    ra, rb, rc, rd = ia * a - ib * c, ia * b - ib * d, idd * c - ic * a, idd * d - ic * b
    sx = math.sqrt(ra * ra + rc * rc)
    if sx > 0.0001:
        det = ra * rd - rb * rc
        return (ax, ay, math.atan2(rc, ra) / DEG, sx, det / sx, 0.0, math.atan2(ra * rb + rc * rd, det) / DEG)
    return (ax, ay, 90 - math.atan2(rd, rb) / DEG, 0.0, math.sqrt(rb * rb + rd * rd), 0.0, 0.0)


def load(json_path, atlas_path, skin=None):
    try:
        data = json.loads(Path(json_path).read_text())
    except (OSError, ValueError) as e:
        fail(f"cannot read {json_path}: {e}")
    return Model(data, read_atlas(atlas_path), skin)


# --- geometry --------------------------------------------------------------------------------

def fit_affine(pairs):
    """Least-squares (a11, a12, a21, a22, t1, t2) with out = A·in + t, and the largest residual."""
    if len(pairs) < 3:
        fail("a picture needs at least three vertices")
    m = [[0.0] * 3 for _ in range(3)]
    rx, ry = [0.0] * 3, [0.0] * 3
    for (X, Y), (x, y) in pairs:
        v = (X, Y, 1.0)
        for i in range(3):
            for j in range(3):
                m[i][j] += v[i] * v[j]
            rx[i] += v[i] * x
            ry[i] += v[i] * y
    cx, cy = solve3(m, rx), solve3(m, ry)
    err = max(math.dist((cx[0] * X + cx[1] * Y + cx[2], cy[0] * X + cy[1] * Y + cy[2]), p) for (X, Y), p in pairs)
    return (cx[0], cx[1], cy[0], cy[1], cx[2], cy[2]), err


def solve3(m, r):
    a = [row[:] + [v] for row, v in zip(m, r)]
    for col in range(3):
        piv = max(range(col, 3), key=lambda i: abs(a[i][col]))
        if abs(a[piv][col]) < 1e-12:
            fail("picture vertices are collinear; no affine fit")
        a[col], a[piv] = a[piv], a[col]
        for i in range(3):
            if i != col:
                f = a[i][col] / a[col][col]
                a[i] = [x - f * y for x, y in zip(a[i], a[col])]
    return [a[i][3] / a[i][i] for i in range(3)]


def inverse_apply(w, p):
    a, b, c, d, x, y = w
    det = a * d - b * c
    dx, dy = p[0] - x, p[1] - y
    return ((d * dx - b * dy) / det, (a * dy - c * dx) / det)


# --- conversion ------------------------------------------------------------------------------

def clip_file_name(name):
    return re.sub(r"_+", "_", re.sub(r"[^a-z0-9_]", "_", name.lower())).strip("_") or "clip"


class Converter:
    def __init__(self, model, image, scale, fps, loop):
        self.m, self.image, self.s, self.fps, self.loop = model, image, scale, fps, loop
        m = model
        self.baked = sorted({b for c in m.constraints for b in c["bones"]})
        self.baked_by = {m.bone_names[b]: [c["name"] for c in m.constraints if b in c["bones"]] for b in self.baked}
        self.setup_worlds = m.pose(None, 0.0)
        self.slot_part = {s["name"]: f"slot:{s['name']}" for s in m.slots}
        clash = set(self.slot_part.values()) & set(m.bone_names)
        if clash:
            fail(f"part id clash between a bone and a slot part: {sorted(clash)}")
        self._fit_pictures()

    def _fit_pictures(self):
        m = self.m
        self.drawn_on, self.fits, self.rigid, self.exact_err = {}, {}, [], {}
        for slot, pic in m.pictures.items():
            totals = {}
            for _, binding in pic["verts"]:
                for bone, _, _, wt in binding:
                    totals[bone] = totals.get(bone, 0.0) + wt
            bone = max(totals, key=lambda b: (totals[b], -b))
            single = all(len(bd) == 1 and bd[0][0] == bone for _, bd in pic["verts"])
            if single:
                pairs = [(page, (bd[0][1], bd[0][2])) for page, bd in pic["verts"]]
            else:
                pairs = [(page, inverse_apply(self.setup_worlds[bone], world))
                         for page, world in m.vertices(slot, self.setup_worlds)]
            affine, err = fit_affine(pairs)
            self.drawn_on[slot] = bone
            self.fits[slot] = affine
            if single:
                self.exact_err[slot] = err
            else:
                total = sum(totals.values())
                self.rigid.append({
                    "slot": slot, "attachment": m.attachment[slot], "bone": m.bone_names[bone],
                    "weights": {m.bone_names[b]: round(v / total, 4) for b, v in sorted(totals.items(), key=lambda kv: -kv[1])},
                    "fit_error": err})

    def pivots(self, origin):
        m, s = self.m, self.s
        piv = []
        for i, (x0, y0, *_ ) in enumerate(m.setup):
            base = origin if m.parent[i] < 0 else piv[m.parent[i]]
            piv.append((base[0] + s * x0, base[1] - s * y0))
        return piv

    def shape(self, slot, piv):
        a11, a12, a21, a22, t1, t2 = self.fits[slot]
        r = self.m.pictures[slot]["region"]
        sw, sh = (r["h"], r["w"]) if r["rotate"] else (r["w"], r["h"])
        X, Y = r["x"], r["y"]
        px, py = piv[self.drawn_on[slot]]
        s = self.s
        return {"image": self.image, "src": [X, Y, sw, sh],
                "transform": [s * a11, -s * a21, s * a12, -s * a22,
                              px + s * (a11 * X + a12 * Y + t1), py - s * (a21 * X + a22 * Y + t2)]}

    def extent(self, poses):
        """[x0, y0, x1, y1] of every shown picture's corners over `poses`, root at the origin."""
        m, s = self.m, self.s
        lo, hi = [math.inf, math.inf], [-math.inf, -math.inf]
        for worlds, visible in poses:
            for slot, (a11, a12, a21, a22, t1, t2) in self.fits.items():
                if not visible[slot]:
                    continue
                r = m.pictures[slot]["region"]
                a, b, c, d, wx, wy = worlds[self.drawn_on[slot]]
                for px, py in ((0, 0), (r["w"], 0), (0, r["h"]), (r["w"], r["h"])):
                    X, Y = page_px(r, px, py)
                    lx, ly = a11 * X + a12 * Y + t1, a21 * X + a22 * Y + t2
                    x, y = s * (a * lx + b * ly + wx), -s * (c * lx + d * ly + wy)
                    lo = [min(lo[0], x), min(lo[1], y)]
                    hi = [max(hi[0], x), max(hi[1], y)]
        return lo + hi if self.fits else [0.0, 0.0, 0.0, 0.0]

    def box(self):
        """The drawing box: the setup pose, centred on the root with the root on its bottom edge,
        because the world mirrors a flipped entity within its box and stands it on the box's
        bottom. Poses reach outside it; `reach` says how far, per clip."""
        m = self.m
        x0, y0, x1, y1 = self.extent([(self.setup_worlds, m.visible(None, 0.0))])
        rx, ry = self.s * m.setup[0][0], -self.s * m.setup[0][1]
        half = max(1, math.ceil(max(rx - x0, x1 - rx)))
        height = max(1, math.ceil(ry - y0))
        origin = (half - rx, height - ry)
        size = (2 * half, height)
        reach = {}
        for name, anim in m.anims.items():
            steps = max(1, math.ceil(anim["duration"] * 30))
            times = [anim["duration"] * k / steps for k in range(steps + 1)]
            e = self.extent([(m.pose(name, t), m.visible(name, t)) for t in times])
            reach[name] = [round(e[0] + origin[0], 1), round(e[1] + origin[1], 1),
                           round(e[2] + origin[0], 1), round(e[3] + origin[1], 1)]
        return origin, size, reach

    def setup_tracks(self, i):
        x0, y0, r0, sx0, sy0 = self.m.setup[i]
        out = {}
        if r0 != 0:
            out["rot"] = [[0, -r0]]
        if sx0 != 1 or sy0 != 1:
            out["scale_x"], out["scale_y"] = [[0, sx0]], [[0, sy0]]
        return out

    def authored(self, i, tls):
        x0, y0, r0, sx0, sy0 = self.m.setup[i]
        s = self.s
        out = self.setup_tracks(i)
        for kind, keys in tls.items():
            if kind == "rotate":
                rows, u, prev = [], None, None
                for t, (r,), _ in keys:
                    u = r if u is None else u + wrap180(r - prev)
                    prev = r
                    rows.append((t, [-(r0 + u)]))
                props, rest = ["rot"], [-r0]
            elif kind == "translate":
                rows = [(t, [s * v[0], -s * v[1]]) for t, v, _ in keys]
                props, rest = ["x", "y"], [0.0, 0.0]
            else:
                rows = [(t, [sx0 * v[0], sy0 * v[1]]) for t, v, _ in keys]
                props, rest = ["scale_x", "scale_y"], [sx0, sy0]
            for j, prop in enumerate(props):
                track = []
                if keys[0][0] > 0:
                    track.append([0, rest[j]])
                for k, (t, values) in enumerate(rows):
                    e = easing(keys[k - 1][2]) if k > 0 else ("step" if track else None)
                    track.append([t, values[j]] + ([e] if e is not None else []))
                out[prop] = track
        return out

    def bake(self, name, length):
        m, s = self.m, self.s
        n = int(math.floor(length * self.fps + 1e-9))
        times = [k / self.fps for k in range(n + 1)]
        if length - times[-1] > 1e-9:
            times.append(length)
        rows = {b: [] for b in self.baked}
        shear = 0.0
        for t in times:
            worlds = m.pose(name, t)
            for b in self.baked:
                p = m.parent[b]
                ax, ay, rot, sx, sy, _, shy = applied(worlds[p] if p >= 0 else None, worlds[b])
                shear = max(shear, abs(wrap180(shy)))
                rows[b].append((t, ax, ay, rot, sx, sy))
        out = {}
        for b in self.baked:
            x0, y0 = m.setup[b][:2]
            tracks = {"x": [], "y": [], "rot": [], "scale_x": [], "scale_y": []}
            prev = None
            for t, ax, ay, rot, sx, sy in rows[b]:
                rot = rot if prev is None else prev + wrap180(rot - prev)
                prev = rot
                for prop, v in (("x", s * (ax - x0)), ("y", -s * (ay - y0)), ("rot", -rot), ("scale_x", sx), ("scale_y", sy)):
                    tracks[prop].append([t, v])
            for prop, keys in tracks.items():
                if all(abs(k[1] - keys[0][1]) < 1e-9 for k in keys):
                    tracks[prop] = [[0, keys[0][1]]]
            out[m.bone_names[b]] = tracks
        return out, shear

    def show_tracks(self, keys_by_slot):
        m = self.m
        out = {}
        for s in m.slots:
            att = m.attachment.get(s["name"])
            if att is None:
                continue
            setup = s.get("attachment") == att
            keys = keys_by_slot.get(s["name"])
            if not keys:
                if not setup:
                    out[self.slot_part[s["name"]]] = {"show": [[0, False]]}
                continue
            track = [] if keys[0][0] == 0 else [[0, setup]]
            track += [[t, name is not None and name == att] for t, name in keys]
            out[self.slot_part[s["name"]]] = {"show": track}
        return out

    def clip(self, name):
        m = self.m
        anim = m.anims[name]
        length = anim["duration"] if anim["duration"] > 0 else 1 / self.fps
        tracks, replaced = {}, {}
        for i, bone in enumerate(m.bone_names):
            tls = anim["bones"].get(i, {})
            if i in self.baked:
                if tls:
                    replaced[bone] = sum(len(k) for k in tls.values())
                continue
            t = self.authored(i, tls)
            if t:
                tracks[bone] = t
        baked, shear = self.bake(name, length)
        tracks.update(baked)
        tracks.update(self.show_tracks(anim["slots"]))
        return {"length": length, "loop": name in self.loop, "tracks": tracks}, replaced, shear

    def setup_clip(self):
        tracks = {}
        for i, bone in enumerate(self.m.bone_names):
            t = self.setup_tracks(i)
            if t:
                tracks[bone] = t
        worlds = self.setup_worlds
        for b in self.baked:
            p = self.m.parent[b]
            ax, ay, rot, sx, sy, _, _ = applied(worlds[p] if p >= 0 else None, worlds[b])
            x0, y0 = self.m.setup[b][:2]
            tracks[self.m.bone_names[b]] = {"x": [[0, self.s * (ax - x0)]], "y": [[0, -self.s * (ay - y0)]],
                                            "rot": [[0, -rot]], "scale_x": [[0, sx]], "scale_y": [[0, sy]]}
        tracks.update(self.show_tracks({}))
        return {"length": 1 / self.fps, "loop": False, "tracks": tracks}

    def rigid_deviation(self):
        """Per rigid-drawn slot: the farthest a vertex strays from Spine's skinned position, over
        every clip at 30 Hz, in drawing units."""
        m = self.m
        worst = {r["slot"]: (0.0, None, None) for r in self.rigid}
        for name, anim in m.anims.items():
            steps = max(1, math.ceil(anim["duration"] * 30))
            for k in range(steps + 1):
                t = anim["duration"] * k / steps
                worlds = m.pose(name, t)
                for slot in worst:
                    a11, a12, a21, a22, t1, t2 = self.fits[slot]
                    a, b, c, d, wx, wy = worlds[self.drawn_on[slot]]
                    for (X, Y), p in m.vertices(slot, worlds):
                        lx, ly = a11 * X + a12 * Y + t1, a21 * X + a22 * Y + t2
                        e = self.s * math.dist((a * lx + b * ly + wx, c * lx + d * ly + wy), p)
                        if e > worst[slot][0]:
                            worst[slot] = (e, name, round(t, 4))
        return worst

    def curve_gap(self):
        """How far Spine's own runtime (curves as 10 segments) puts a bone from the exact curve
        the engine plays, over every clip at 30 Hz, in drawing units."""
        gap = 0.0
        for name, anim in self.m.anims.items():
            steps = max(1, math.ceil(anim["duration"] * 30))
            for k in range(steps + 1):
                t = anim["duration"] * k / steps
                exact, approx = self.m.pose(name, t), self.m.pose(name, t, approx=True)
                gap = max(gap, max(math.dist(e[4:], a[4:]) for e, a in zip(exact, approx)) * self.s)
        return gap


# --- Lua output ------------------------------------------------------------------------------

LUA_KEYWORDS = set("and break do else elseif end false for function if in local nil not or repeat return then "
                   "true until while continue".split())


def num(v):
    r = round(float(v), 6)
    if r == 0:
        return "0"
    if r == int(r) and abs(r) < 1e15:
        return str(int(r))
    return format(r, ".10g")


def lua_key(k):
    if re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", k) and k not in LUA_KEYWORDS:
        return k
    return "[" + json.dumps(k) + "]"


def lua_value(v):
    if isinstance(v, bool):
        return "true" if v else "false"
    if isinstance(v, (int, float)):
        return num(v)
    if isinstance(v, str):
        return json.dumps(v)
    return "{ " + ", ".join(lua_value(x) for x in v) + " }"


def lua_keys(keys):
    out, last = [], None
    for k in keys:
        t = num(k[0])
        if t == last:
            fail(f"two keys at time {t} after rounding")
        last = t
        out.append("{" + ", ".join([t] + [lua_value(x) for x in k[1:]]) + "}")
    return "{ " + ", ".join(out) + " }"


PROPS = ("x", "y", "rot", "scale_x", "scale_y", "show")


def clip_lua(clip, header, order):
    lines = [f"-- {line}" for line in header] + [
        "return {", f"\tlength = {num(clip['length'])},", f"\tloop = {lua_value(clip['loop'])},", "\ttracks = {"]
    for part in sorted(clip["tracks"], key=lambda p: order[p]):
        props = clip["tracks"][part]
        lines.append(f"\t\t{lua_key(part)} = {{")
        for prop in PROPS:
            if prop in props:
                lines.append(f"\t\t\t{prop} = {lua_keys(props[prop])},")
        lines.append("\t\t},")
    lines += ["\t},", "}", ""]
    return "\n".join(lines)


def rig_lua(size, parts, header):
    lines = [f"-- {line}" for line in header] + ["return {", f"\tsize = {lua_value(size)},", "\tparts = {"]
    for p in parts:
        head = f"id = {json.dumps(p['id'])}"
        if p.get("parent"):
            head += f", parent = {json.dumps(p['parent'])}"
        head += f", pivot = {lua_value(p['pivot'])}"
        if "shapes" not in p:
            lines.append(f"\t\t{{ {head} }},")
            continue
        lines.append(f"\t\t{{ {head}, shapes = {{")
        for sh in p["shapes"]:
            lines.append(f"\t\t\t{{ image = {json.dumps(sh['image'])}, src = {lua_value(sh['src'])},")
            lines.append(f"\t\t\t  transform = {lua_value(sh['transform'])} }},")
        lines.append("\t\t} },")
    lines += ["\t},", "}", ""]
    return "\n".join(lines)


def convert(json_path, atlas_path, out, skin=None, image=None, scale=1.0, fps=60, loop=()):
    m = load(json_path, atlas_path, skin)
    out = Path(out)
    image = image or f"characters/{out.name}/atlas.png"
    loop = set(loop)
    missing = loop - set(m.anims)
    if missing:
        fail(f"--loop names unknown animations {sorted(missing)}")
    conv = Converter(m, image, scale, fps, loop)
    origin, size, reach = conv.box()
    piv = conv.pivots(origin)

    parts = []
    for i, name in enumerate(m.bone_names):
        part = {"id": name, "pivot": [piv[i][0], piv[i][1]]}
        if m.parent[i] >= 0:
            part["parent"] = m.bone_names[m.parent[i]]
        parts.append(part)
    empty = []
    for s in m.slots:
        bone = conv.drawn_on.get(s["name"], m.bone_index[s["bone"]])
        part = {"id": conv.slot_part[s["name"]], "parent": m.bone_names[bone], "pivot": list(piv[bone])}
        if s["name"] in m.pictures:
            part["shapes"] = [conv.shape(s["name"], piv)]
        else:
            empty.append(s["name"])
        parts.append(part)
    order = {p["id"]: k for k, p in enumerate(parts)}

    clips, files, max_shear, replaced = [], {}, 0.0, {}
    for name in m.anims:
        file = clip_file_name(name)
        if file in files:
            fail(f"animations {files[file]!r} and {name!r} both become clips/{file}.lua")
        files[file] = name
        clip, rep, shear = conv.clip(name)
        max_shear = max(max_shear, shear)
        if rep:
            replaced[name] = rep
        clips.append((name, file, clip))
    setup = conv.setup_clip()

    rigid_dev = conv.rigid_deviation()
    for r in conv.rigid:
        dev, at_clip, at_t = rigid_dev[r["slot"]]
        r["fit_error"] = round(r["fit_error"] * scale, 4)
        r["max_deviation"] = {"distance": round(dev, 3), "clip": at_clip, "time": at_t}
    gap = conv.curve_gap()

    src = Path(json_path).name
    header = [f"{src} (Spine {m.data['skeleton']['spine']}), skin {m.skin!r}, converted by scripts/spine_to_lua.py."]
    report = {
        "input": src, "spine": m.data["skeleton"]["spine"], "skin": m.skin, "atlas_page": m.page,
        "image": image, "scale": scale, "fps": fps, "origin": list(origin), "size": list(size),
        "reach": reach,
        "slot_parts": conv.slot_part,
        "drawn_on": {s: m.bone_names[b] for s, b in conv.drawn_on.items()},
        "reparented": [{"slot": s["name"], "slot_bone": s["bone"], "drawn_on": m.bone_names[conv.drawn_on[s["name"]]]}
                       for s in m.slots if s["name"] in conv.drawn_on and m.bone_names[conv.drawn_on[s["name"]]] != s["bone"]],
        "empty_slots": empty,
        "baked": [m.bone_names[b] for b in conv.baked],
        "baked_by": conv.baked_by,
        "baked_max_shear_degrees": round(max_shear, 6),
        "replaced_keys": replaced,
        "rigid": conv.rigid,
        "exact_fit_error": {s: round(e * scale, 6) for s, e in conv.exact_err.items()},
        "curve_runtime_gap": round(gap, 4),
        "clips": [{"name": n, "file": f, "length": c["length"], "loop": c["loop"],
                   "keys": sum(len(k) for p in c["tracks"].values() for k in p.values())} for n, f, c in clips],
        "approximations": [
            "meshes skinned to several bones are drawn rigid on their heaviest bone (see rigid)",
            "every mesh is drawn as its whole atlas rectangle, not clipped to the mesh hull",
            f"constrained bones are linear between {fps} Hz samples",
            "the engine solves each bezier exactly; Spine's runtime uses 10 straight segments "
            "(curve_runtime_gap is the largest bone offset that makes)",
        ] + ([f"baked bones carry up to {max_shear:.4f} degrees of shear, which the engine drops"]
             if max_shear > 1e-6 else []),
        "ignored": ["skeleton hash/images/audio/bounds", "bone colours; bone lengths outside IK",
                    "mesh triangles/edges/hull and width/height", "attachments of other skins",
                    "loop: Spine data does not say; set with --loop"],
    }

    # Everything is rendered before anything is written, so a failure leaves no half output.
    texts = {"rig.lua": rig_lua(list(size), parts, header + [
        "Rest pose: every bone unrotated and unscaled at its setup offset from its parent; rotation and",
        "scale live in the clips (setup.lua is the setup pose). List order is draw order.",
        "The box is the setup pose, the root bone at its bottom centre; clips reach outside it (report.json: reach)."])}
    for name, file, clip in clips:
        texts[f"clips/{file}.lua"] = clip_lua(clip, [
            f"Spine animation {name!r}, {num(clip['length'])} s. " + header[0],
            "Values are absolute: rot in degrees (y down), scale_x/scale_y, x/y offsets from the rest pivot."], order)
    texts["setup.lua"] = clip_lua(setup, ["The setup pose. " + header[0]], order)
    (out / "clips").mkdir(parents=True, exist_ok=True)
    for rel, text in texts.items():
        (out / rel).write_text(text)
    if m.page:
        shutil.copyfile(Path(atlas_path).parent / m.page, out / "atlas.png")
    stale = sorted(f.name for f in (out / "clips").glob("*.lua") if f.stem not in files)
    if stale:
        report["stale_clip_files"] = stale
    (out / "report.json").write_text(json.dumps(report, indent=1) + "\n")
    return report


def summary(r):
    lines = [f"{r['input']} -> skin {r['skin']}, page {r['atlas_page']}, size {r['size']}, origin {r['origin']}",
             f"clips: {len(r['clips'])} ({sum(c['keys'] for c in r['clips'])} keys)",
             f"baked ({len(r['baked'])}): " + ", ".join(f"{b} [{', '.join(r['baked_by'][b])}]" for b in r["baked"]),
             f"rigid-drawn skinned meshes ({len(r['rigid'])}):"]
    for x in r["rigid"]:
        lines.append(f"  {x['slot']} on {x['bone']}: fit error {x['fit_error']}, worst {x['max_deviation']['distance']}"
                     f" in {x['max_deviation']['clip']} at {x['max_deviation']['time']} s; weights {x['weights']}")
    if r["exact_fit_error"]:
        lines.append(f"one-bone pictures: worst fit error {max(r['exact_fit_error'].values())}")
    for x in r["reparented"]:
        lines.append(f"  slot {x['slot']} is drawn on {x['drawn_on']} (its slot bone is {x['slot_bone']})")
    if r["empty_slots"]:
        lines.append(f"slots with nothing to draw: {', '.join(r['empty_slots'])}")
    if r["replaced_keys"]:
        lines.append("authored keys replaced by baking: " +
                     ", ".join(f"{c}: {sum(v.values())}" for c, v in r["replaced_keys"].items()))
    lines.append(f"curve gap to Spine's runtime: {r['curve_runtime_gap']}")
    lines += [f"approximation: {a}" for a in r["approximations"]]
    if r.get("stale_clip_files"):
        lines.append(f"stale clip files left in place: {r['stale_clip_files']}")
    return "\n".join(lines)


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("json")
    ap.add_argument("atlas")
    ap.add_argument("out")
    ap.add_argument("--skin")
    ap.add_argument("--image", help="the picture path shapes name, relative to the app (default characters/<out>/atlas.png)")
    ap.add_argument("--scale", type=float, default=1.0)
    ap.add_argument("--fps", type=int, default=60, help="samples a second for baked bones")
    ap.add_argument("--loop", default="", help="comma-separated animations to mark loop = true")
    args = ap.parse_args(argv)
    try:
        report = convert(args.json, args.atlas, args.out, args.skin, args.image, args.scale, args.fps,
                         [n for n in args.loop.split(",") if n])
    except ConvertError as e:
        print(f"spine_to_lua: error: {e}", file=sys.stderr)
        return 1
    print(summary(report))
    return 0


if __name__ == "__main__":
    sys.exit(main())
