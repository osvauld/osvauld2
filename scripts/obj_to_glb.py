"""Convert a Wavefront OBJ (+ MTL, + PNG/JPEG maps) into one GLB that `gfx.load_glb` reads.

    python3 scripts/obj_to_glb.py model.obj out.glb [--skip REGEX] [--scale S]

Each OBJ object becomes a node named as in Blender (the exporter's `_<Mesh>.NNN` suffix is
dropped) with its origin moved to the centre of its bounds, so parts like wheels can turn about
themselves. Faces are fan-triangulated, one primitive per material. `Kd` becomes the base colour
(both linear), `map_Kd` an embedded texture. Stdlib only; nothing is decided about what a part
*means* — that stays in the app's Lua.
"""
from pathlib import Path
import argparse
import re
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent / "fixtures"))
from make_glb import Buf, glb  # noqa: E402

BLENDER_SUFFIX = re.compile(r"_(Cube|Cylinder|Plane|Sphere|Circle|Cone|Torus|Mesh|Icosphere)(\.\d+)?$")


def materials(path: Path) -> dict:
    out, cur = {}, None
    if not path.exists():
        return out
    for line in path.read_text(errors="replace").splitlines():
        parts = line.split()
        if not parts:
            continue
        if parts[0] == "newmtl":
            cur = out.setdefault(" ".join(parts[1:]), {"color": [0.8, 0.8, 0.8, 1.0]})
        elif cur is None:
            continue
        elif parts[0] == "Kd":
            cur["color"][:3] = [float(x) for x in parts[1:4]]
        elif parts[0] == "d":
            cur["color"][3] = float(parts[1])
        elif parts[0] == "map_Kd":
            image = path.parent / parts[-1]
            if image.suffix.lower() in (".png", ".jpg", ".jpeg") and image.exists():
                cur["image"] = image
    return out


def convert(src: Path, skip: str | None, scale: float) -> bytes:
    pos, uv, nor = [], [], []
    objects = []  # [name, {material: [(v, vt, vn) * 3, ...]}]
    mtl, material = {}, None
    drop = re.compile(skip) if skip else None
    for line in src.read_text(errors="replace").splitlines():
        parts = line.split()
        if not parts:
            continue
        tag = parts[0]
        if tag == "v":
            pos.append([float(x) * scale for x in parts[1:4]])
        elif tag == "vt":
            u, v = float(parts[1]), float(parts[2]) if len(parts) > 2 else 0.0
            uv.append((u, 1.0 - v))  # OBJ's v runs up, glTF's down
        elif tag == "vn":
            nor.append([float(x) for x in parts[1:4]])
        elif tag == "mtllib":
            mtl.update(materials(src.parent / " ".join(parts[1:])))
        elif tag in ("o", "g"):
            name = BLENDER_SUFFIX.sub("", " ".join(parts[1:]))
            objects.append([name, {}])
        elif tag == "usemtl":
            material = " ".join(parts[1:])
        elif tag == "f":
            if not objects:
                objects.append(["object", {}])
            corners = []
            for c in parts[1:]:
                ids = (c.split("/") + ["", ""])[:3]
                ref = []
                for i, (k, table) in enumerate(zip(ids, (pos, uv, nor))):
                    ref.append(None if k == "" else (int(k) - 1 if int(k) > 0 else len(table) + int(k)))
                corners.append(tuple(ref))
            tris = objects[-1][1].setdefault(material, [])
            for i in range(1, len(corners) - 1):
                tris.append((corners[0], corners[i], corners[i + 1]))

    buf, nodes, meshes, mats, images = Buf(), [], [], [], []
    mat_index = {}
    for name, groups in objects:
        if drop and drop.search(name) or not groups:
            continue
        used = [pos[c[0]] for tris in groups.values() for t in tris for c in t]
        lo = [min(p[i] for p in used) for i in range(3)]
        hi = [max(p[i] for p in used) for i in range(3)]
        centre = [(a + b) / 2 for a, b in zip(lo, hi)]
        prims = []
        for mname, tris in groups.items():
            if mname not in mat_index:
                m = mtl.get(mname, {"color": [0.8, 0.8, 0.8, 1.0]})
                pbr = {"baseColorFactor": m["color"], "metallicFactor": 0}
                if "image" in m:
                    view = buf.raw(m["image"].read_bytes())
                    mime = "image/png" if m["image"].suffix.lower() == ".png" else "image/jpeg"
                    images.append({"bufferView": view, "mimeType": mime})
                    pbr["baseColorTexture"] = {"index": len(images) - 1}
                mat_index[mname] = len(mats)
                mats.append({"name": mname or "default", "pbrMetallicRoughness": pbr})
            seen, P, N, T, I = {}, [], [], [], []
            has_n = all(c[2] is not None for t in tris for c in t)
            has_t = all(c[1] is not None for t in tris for c in t)
            for t in tris:
                for c in t:
                    if c not in seen:
                        seen[c] = len(P)
                        P.append(tuple(pos[c[0]][i] - centre[i] for i in range(3)))
                        if has_n:
                            N.append(tuple(nor[c[2]]))
                        if has_t:
                            T.append(uv[c[1]])
                    I.append(seen[c])
            attrs = {"POSITION": buf.positions(P)}
            if has_n:
                attrs["NORMAL"] = buf.normals(N)
            if has_t:
                attrs["TEXCOORD_0"] = buf.uvs(T)
            prims.append({"attributes": attrs, "material": mat_index[mname],
                          "indices": buf.add("I", I, "SCALAR", 5125, 34963)})
        meshes.append({"name": name, "primitives": prims})
        nodes.append({"name": name, "mesh": len(meshes) - 1, "translation": centre})
    doc = {"scene": 0, "scenes": [{"nodes": list(range(len(nodes)))}], "nodes": nodes,
           "meshes": meshes, "materials": mats, "bufferViews": buf.views, "accessors": buf.accessors}
    if images:
        doc["images"] = images
        doc["textures"] = [{"source": i} for i in range(len(images))]
    return glb(doc, buf.data)


if __name__ == "__main__":
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("obj", type=Path)
    ap.add_argument("out", type=Path)
    ap.add_argument("--skip", help="regex of object names to leave out (studio floors, backdrops)")
    ap.add_argument("--scale", type=float, default=1.0)
    a = ap.parse_args()
    a.out.parent.mkdir(parents=True, exist_ok=True)
    a.out.write_bytes(convert(a.obj, a.skip, a.scale))
    print("wrote", a.out, a.out.stat().st_size, "bytes")
