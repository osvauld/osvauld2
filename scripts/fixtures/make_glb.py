"""Write the small deterministic GLB fixtures in scripts/fixtures/glb/ (stdlib only).

Run from the repo root: `python3 scripts/fixtures/make_glb.py`. The outputs are checked in; a
test failing on a fixture is fixed here, then regenerated. Each file names what it exercises.
"""
from pathlib import Path
import json
import struct
import zlib

OUT = Path(__file__).resolve().parent / "glb"


def glb(doc: dict, binary: bytes) -> bytes:
    binary += b"\0" * (-len(binary) % 4)
    doc = dict(doc, asset={"version": "2.0", "generator": "osvauld make_glb.py"})
    if binary:
        doc["buffers"] = [{"byteLength": len(binary)}]
    js = json.dumps(doc, separators=(",", ":"), sort_keys=True).encode()
    js += b" " * (-len(js) % 4)
    chunks = struct.pack("<II", len(js), 0x4E4F534A) + js
    if binary:
        chunks += struct.pack("<II", len(binary), 0x004E4942) + binary
    return struct.pack("<III", 0x46546C67, 2, 12 + len(chunks)) + chunks


def png(width: int, height: int, pixel) -> bytes:
    """An RGBA8 PNG; `pixel(x, y)` gives (r, g, b, a)."""
    def chunk(kind: bytes, data: bytes) -> bytes:
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))
    rows = b"".join(b"\0" + b"".join(bytes(pixel(x, y)) for x in range(width)) for y in range(height))
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(rows, 9)) + chunk(b"IEND", b""))


class Buf:
    def __init__(self):
        self.data = b""
        self.views, self.accessors = [], []

    def add(self, fmt: str, rows, kind: str, ctype: int, target: int | None = None, minmax=False):
        start = len(self.data)
        packed = b"".join(struct.pack("<" + fmt, *(r if isinstance(r, tuple) else (r,))) for r in rows)
        self.data += packed + b"\0" * (-len(packed) % 4)
        view = {"buffer": 0, "byteOffset": start, "byteLength": len(packed)}
        if target:
            view["target"] = target
        self.views.append(view)
        acc = {"bufferView": len(self.views) - 1, "componentType": ctype, "count": len(rows), "type": kind}
        if minmax:
            cols = list(zip(*rows))
            acc["min"], acc["max"] = [min(c) for c in cols], [max(c) for c in cols]
        self.accessors.append(acc)
        return len(self.accessors) - 1

    def positions(self, rows):
        return self.add("fff", rows, "VEC3", 5126, 34962, minmax=True)

    def normals(self, rows):
        return self.add("fff", rows, "VEC3", 5126, 34962)

    def indices(self, rows):
        return self.add("H", rows, "SCALAR", 5123, 34963)

    def uvs(self, rows):
        return self.add("ff", rows, "VEC2", 5126, 34962)

    def raw(self, data: bytes) -> int:
        """A bufferView of opaque bytes (an embedded image)."""
        start = len(self.data)
        self.data += data + b"\0" * (-len(data) % 4)
        self.views.append({"buffer": 0, "byteOffset": start, "byteLength": len(data)})
        return len(self.views) - 1


def cube(buf: Buf, normals: bool, degenerate: bool):
    """A unit cube; `degenerate` appends a zero-area triangle an exporter might leave."""
    faces = [((0, 0, 1), (1, 0, 0), (0, 1, 0)), ((0, 0, -1), (-1, 0, 0), (0, 1, 0)),
             ((1, 0, 0), (0, 0, -1), (0, 1, 0)), ((-1, 0, 0), (0, 0, 1), (0, 1, 0)),
             ((0, 1, 0), (1, 0, 0), (0, 0, -1)), ((0, -1, 0), (1, 0, 0), (0, 0, 1))]
    pos, nor, idx = [], [], []
    for n, u, v in faces:
        base = len(pos)
        for su, sv in ((-1, -1), (1, -1), (1, 1), (-1, 1)):
            pos.append(tuple(0.5 * (n[i] + su * u[i] + sv * v[i]) for i in range(3)))
            nor.append(n)
        idx += [base, base + 1, base + 2, base, base + 2, base + 3]
    if degenerate:
        idx += [0, 0, 1]
    attrs = {"POSITION": buf.positions(pos)}
    if normals:
        attrs["NORMAL"] = buf.normals(nor)
    return {"attributes": attrs, "indices": buf.indices(idx), "material": 0}


def hierarchy() -> bytes:
    """Two meshes, a parent/child pair with extras, one material; the child's cube has no
    normals and one degenerate triangle."""
    buf = Buf()
    doc = {
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": [
            {"name": "base", "mesh": 0, "children": [1], "translation": [0, 0.5, 0],
             "extras": {"collider": "box", "mass": 120, "tags": ["static", "metal"]}},
            {"name": "arm", "mesh": 1, "translation": [0, 1, 0],
             "rotation": [0, 0.7071068, 0, 0.7071068], "scale": [0.5, 2, 0.5]},
            {"name": "marker"},
        ],
        "meshes": [{"primitives": [cube(buf, True, False)]}, {"primitives": [cube(buf, False, True)]}],
        "materials": [{"name": "paint", "pbrMetallicRoughness": {"baseColorFactor": [1, 0.2158605, 0, 1]}}],
    }
    doc["bufferViews"], doc["accessors"] = buf.views, buf.accessors
    return glb(doc, buf.data)


def quad_with_image(image: bytes) -> bytes:
    """A 2x1 quad facing +Z whose base colour comes from `image` through TEXCOORD_0."""
    buf = Buf()
    pos = [(-1, -0.5, 0), (1, -0.5, 0), (1, 0.5, 0), (-1, 0.5, 0)]
    prim = {"attributes": {"POSITION": buf.positions(pos), "NORMAL": buf.normals([(0, 0, 1)] * 4),
                           "TEXCOORD_0": buf.uvs([(0, 1), (1, 1), (1, 0), (0, 0)])},
            "indices": buf.indices([0, 1, 2, 0, 2, 3]), "material": 0}
    doc = {
        "scene": 0, "scenes": [{"nodes": [0]}],
        "nodes": [{"name": "panel", "mesh": 0, "extras": {"kind": "sign"}}],
        "meshes": [{"primitives": [prim]}],
        "materials": [{"name": "painted", "pbrMetallicRoughness": {
            "baseColorFactor": [1, 1, 1, 1], "baseColorTexture": {"index": 0}}}],
        "textures": [{"source": 0}],
        "images": [{"bufferView": buf.raw(image), "mimeType": "image/png"}],
    }
    doc["bufferViews"], doc["accessors"] = buf.views, buf.accessors
    return glb(doc, buf.data)


def textured() -> bytes:
    """Left half red, right half blue: pixels prove the texture, not the flat colour, drew."""
    return quad_with_image(png(8, 4, lambda x, y: (255, 0, 0, 255) if x < 4 else (0, 0, 255, 255)))


def oversize_image() -> bytes:
    """A valid 4097x1 PNG: refused from its header, one past the 2048 side limit."""
    return quad_with_image(png(4097, 1, lambda x, y: (0, 0, 0, 255)))


def external_buffer() -> bytes:
    """Refers to a sidecar .bin: must be refused, never fetched."""
    doc = {"buffers": [{"byteLength": 12, "uri": "mesh.bin"}],
           "bufferViews": [{"buffer": 0, "byteLength": 12}],
           "accessors": [{"bufferView": 0, "componentType": 5126, "count": 1, "type": "VEC3",
                          "min": [0, 0, 0], "max": [0, 0, 0]}],
           "meshes": [{"primitives": [{"attributes": {"POSITION": 0}}]}],
           "nodes": [{"mesh": 0}], "scenes": [{"nodes": [0]}]}
    out = glb(doc, b"")
    return out


FIXTURES = {"hierarchy.glb": hierarchy, "external_buffer.glb": external_buffer,
            "textured.glb": textured, "oversize_image.glb": oversize_image}

if __name__ == "__main__":
    OUT.mkdir(exist_ok=True)
    for name, make in FIXTURES.items():
        (OUT / name).write_bytes(make())
        print("wrote", OUT / name)
