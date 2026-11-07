"""Write demo_apps/moon_rover/models/moon_surface.glb: a ~200k-vertex lunar terrain (stdlib only).

Stands in for a Blender export until a real one arrives: one textured `ground` mesh, `boulder-N`
meshes, and an empty `spawn` node. Nodes the app should collide with carry `extras.body = "mesh"`;
what that means is the app's rule. Deterministic; generated, not checked in (~11 MB):
`python3 scripts/fixtures/make_moon.py [OUT]`.
"""
from pathlib import Path
import math
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
from make_glb import Buf, glb, png  # noqa: E402

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "demo_apps" / "moon_rover" / "models" / "moon_surface.glb"
N = 449  # vertices per side: 201,601 in all
STEP = 0.5  # metres between vertices; the ground spans 224 m
TILE = 8.0  # metres per texture repeat
FLAT = 8.0  # radius kept level around the spawn


class Rng:
    def __init__(self, seed):
        self.s = seed

    def __call__(self):
        self.s = (self.s * 48271) % 2147483647
        return self.s / 2147483647


def lattice(ix, iz, seed, period=None):
    if period:
        ix, iz = ix % period, iz % period
    h = (ix * 374761393 + iz * 668265263 + seed * 2147483647) & 0xFFFFFFFF
    h = ((h ^ (h >> 13)) * 1274126177) & 0xFFFFFFFF
    return ((h ^ (h >> 16)) & 0xFFFF) / 65535.0


def noise(x, z, seed, period=None):
    """Smooth value noise in [0, 1]; tileable when `period` (in lattice cells) is given."""
    ix, iz = math.floor(x), math.floor(z)
    fx, fz = x - ix, z - iz
    fx, fz = fx * fx * (3 - 2 * fx), fz * fz * (3 - 2 * fz)
    a, b = lattice(ix, iz, seed, period), lattice(ix + 1, iz, seed, period)
    c, d = lattice(ix, iz + 1, seed, period), lattice(ix + 1, iz + 1, seed, period)
    return a + (b - a) * fx + (c - a) * fz + (a - b - c + d) * fx * fz


def smooth(a, b, x):
    t = min(max((x - a) / (b - a), 0.0), 1.0)
    return t * t * (3 - 2 * t)


def craters(rng, count=110):
    out = []
    half = (N - 1) * STEP / 2
    while len(out) < count:
        radius = min(1.5 * rng() ** (-1 / 1.6), 22.0)
        x, z = (rng() * 2 - 1) * half, (rng() * 2 - 1) * half
        if math.hypot(x, z) < FLAT + radius * 1.5:
            continue
        fresh = 0.35 + 0.65 * rng()
        depth = radius * (0.28 if radius < 6 else 0.2) * fresh
        out.append((x, z, radius, depth, radius * 0.09 * fresh))
    return out


def heights():
    h = [[0.0] * N for _ in range(N)]
    half = (N - 1) * STEP / 2
    for row in range(N):
        z = row * STEP - half
        for col in range(N):
            x = col * STEP - half
            y = (4.0 * noise(x / 60, z / 60, 1) + 1.0 * noise(x / 17, z / 17, 2)
                 + 0.18 * noise(x / 5, z / 5, 3) + 0.05 * noise(x / 1.3, z / 1.3, 4))
            h[row][col] = y
    for cx, cz, radius, depth, rim in craters(Rng(1969)):
        reach = radius * 2.5
        c0, c1 = max(0, int((cx - reach + half) / STEP)), min(N - 1, int((cx + reach + half) / STEP) + 1)
        r0, r1 = max(0, int((cz - reach + half) / STEP)), min(N - 1, int((cz + reach + half) / STEP) + 1)
        for row in range(r0, r1 + 1):
            for col in range(c0, c1 + 1):
                t = math.hypot(col * STEP - half - cx, row * STEP - half - cz) / radius
                if t < 1:
                    floor = 0.25
                    bowl = rim - (depth + rim) * (1 - smooth(floor, 1.0, t) ** 0.8 if t > floor else 1)
                    h[row][col] += bowl
                elif t < 2.5:
                    h[row][col] += rim * (1 / t) ** 3 * (1 - smooth(2.0, 2.5, t))
    centre = h[(N - 1) // 2][(N - 1) // 2]
    for row in range(N):
        for col in range(N):
            r = math.hypot(col * STEP - half, row * STEP - half)
            k = smooth(FLAT * 0.6, FLAT * 1.6, r)
            h[row][col] = centre + (h[row][col] - centre) * k - centre
    return h


def sample(h, x, z):
    half = (N - 1) * STEP / 2
    u, v = (x + half) / STEP, (z + half) / STEP
    col, row = min(int(u), N - 2), min(int(v), N - 2)
    fx, fz = u - col, v - row
    a, b, c, d = h[row][col], h[row][col + 1], h[row + 1][col], h[row + 1][col + 1]
    return a + (b - a) * fx + (c - a) * fz + (a - b - c + d) * fx * fz


def regolith() -> bytes:
    size, cells = 512, [64, 32, 16, 8, 4]

    def pixel(x, y):
        v = 0.0
        for i, period in enumerate(cells):
            k = size // period
            v += noise(x / k, y / k, 10 + i, period) * (0.5 ** i)
        v = v / 1.94
        speck = lattice(x, y, 99)
        if speck > 0.985:
            v += 0.25
        elif speck < 0.02:
            v -= 0.2
        g = max(0, min(255, int(70 + v * 120)))
        return (g, g - 2 if g > 2 else 0, g - 5 if g > 5 else 0, 255)

    return png(size, size, pixel)


def ground(buf: Buf, h):
    half = (N - 1) * STEP / 2
    pos, nor, uv, idx = [], [], [], []
    for row in range(N):
        for col in range(N):
            x, z = col * STEP - half, row * STEP - half
            l, r = h[row][max(col - 1, 0)], h[row][min(col + 1, N - 1)]
            b, f = h[max(row - 1, 0)][col], h[min(row + 1, N - 1)][col]
            dx, dz = (r - l) / (2 * STEP), (f - b) / (2 * STEP)
            n = math.sqrt(dx * dx + 1 + dz * dz)
            pos.append((x, h[row][col], z))
            nor.append((-dx / n, 1 / n, -dz / n))
            uv.append((x / TILE, z / TILE))
            if row < N - 1 and col < N - 1:
                a = row * N + col
                idx += [a, a + N, a + 1, a + 1, a + N, a + N + 1]
    return {"attributes": {"POSITION": buf.positions(pos), "NORMAL": buf.normals(nor),
                           "TEXCOORD_0": buf.uvs(uv)},
            "indices": buf.add("I", idx, "SCALAR", 5125, 34963), "material": 0}


def icosphere():
    t = (1 + 5 ** 0.5) / 2
    v = [(-1, t, 0), (1, t, 0), (-1, -t, 0), (1, -t, 0), (0, -1, t), (0, 1, t),
         (0, -1, -t), (0, 1, -t), (t, 0, -1), (t, 0, 1), (-t, 0, -1), (-t, 0, 1)]
    f = [(0, 11, 5), (0, 5, 1), (0, 1, 7), (0, 7, 10), (0, 10, 11), (1, 5, 9), (5, 11, 4),
         (11, 10, 2), (10, 7, 6), (7, 1, 8), (3, 9, 4), (3, 4, 2), (3, 2, 6), (3, 6, 8),
         (3, 8, 9), (4, 9, 5), (2, 4, 11), (6, 2, 10), (8, 6, 7), (9, 8, 1)]
    n = math.sqrt(1 + t * t)
    return [(a / n, b / n, c / n) for a, b, c in v], f


def boulder(buf: Buf, rng, radius):
    """A flat-shaded lumpy rock in its node's frame, resting with its base slightly sunk."""
    verts, faces = icosphere()
    yaw = rng() * math.tau
    cy, sy = math.cos(yaw), math.sin(yaw)
    pts = []
    for x, y, z in verts:
        k = radius * (0.75 + 0.5 * rng())
        x, y, z = x * k * 1.2, y * k * 0.7, z * k
        pts.append((x * cy + z * sy, y, -x * sy + z * cy))
    pos, nor, uv = [], [], []
    for a, b, c in faces:
        p, q, r = pts[a], pts[b], pts[c]
        u = [q[i] - p[i] for i in range(3)]
        w = [r[i] - p[i] for i in range(3)]
        n = (u[1] * w[2] - u[2] * w[1], u[2] * w[0] - u[0] * w[2], u[0] * w[1] - u[1] * w[0])
        if sum(n[i] * (p[i] + q[i] + r[i]) for i in range(3)) < 0:
            q, r, n = r, q, tuple(-x for x in n)
        m = math.sqrt(sum(x * x for x in n))
        for s in (p, q, r):
            pos.append(s)
            nor.append(tuple(x / m for x in n))
            uv.append((s[0] + s[1], s[2] + s[1]))
    return {"attributes": {"POSITION": buf.positions(pos), "NORMAL": buf.normals(nor),
                           "TEXCOORD_0": buf.uvs(uv)}, "material": 1}


def moon_surface() -> bytes:
    buf = Buf()
    h = heights()
    image = buf.raw(regolith())
    meshes = [{"name": "ground", "primitives": [ground(buf, h)]}]
    nodes = [{"name": "ground", "mesh": 0, "extras": {"body": "mesh"}},
             {"name": "spawn", "translation": [0, sample(h, 0, 0), 0], "extras": {"role": "spawn"}}]
    rng, half = Rng(4242), (N - 1) * STEP / 2 - 4
    while len(meshes) < 41:
        x, z = (rng() * 2 - 1) * half, (rng() * 2 - 1) * half
        if math.hypot(x, z) < FLAT + 4:
            continue
        radius = 0.3 + 1.1 * rng() ** 2
        meshes.append({"name": f"boulder-{len(meshes)}", "primitives": [boulder(buf, rng, radius)]})
        nodes.append({"name": f"boulder-{len(meshes) - 1}", "mesh": len(meshes) - 1,
                      "translation": [x, sample(h, x, z) + radius * 0.25, z], "extras": {"body": "mesh"}})
    doc = {"scene": 0, "scenes": [{"nodes": list(range(len(nodes)))}], "nodes": nodes, "meshes": meshes,
           "materials": [{"name": "regolith", "pbrMetallicRoughness": {"baseColorTexture": {"index": 0},
                                                                         "metallicFactor": 0}},
                         {"name": "basalt", "pbrMetallicRoughness": {
                             "baseColorFactor": [0.55, 0.54, 0.52, 1], "baseColorTexture": {"index": 0},
                             "metallicFactor": 0}}],
           "textures": [{"source": 0, "sampler": 0}], "samplers": [{"wrapS": 10497, "wrapT": 10497}],
           "images": [{"bufferView": image, "mimeType": "image/png"}],
           "bufferViews": buf.views, "accessors": buf.accessors}
    return glb(doc, buf.data)


if __name__ == "__main__":
    out = Path(sys.argv[1]) if len(sys.argv) > 1 else OUT
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_bytes(moon_surface())
    print("wrote", out, out.stat().st_size, "bytes")
