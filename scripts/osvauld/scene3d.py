"""Pixel gates for 3D bridge smokes: inspected geometry must also reach the shell renderer."""
import base64
import math

from .png import Image


def project(camera, world, rect):
    """Project an inspected world point into a resolved logical viewport."""
    def dot(a, b):
        return sum(x * y for x, y in zip(a, b))

    def unit(a):
        length = math.sqrt(dot(a, a))
        return [v / length for v in a]

    def cross(a, b):
        return [a[1]*b[2]-a[2]*b[1], a[2]*b[0]-a[0]*b[2], a[0]*b[1]-a[1]*b[0]]

    eye = camera["eye"]
    forward = unit([t-e for t, e in zip(camera["target"], eye)])
    right = unit(cross(forward, camera["up"]))
    up = cross(right, forward)
    delta = [p-e for p, e in zip(world, eye)]
    extent = dot(delta, forward) * math.tan(camera["fov_y_radians"] / 2)
    assert extent > 0, "pixel probe is behind the camera"
    x = dot(delta, right) / (extent * rect["w"] / rect["h"])
    y = dot(delta, up) / extent
    return rect["x"] + (x+1)*rect["w"]/2, rect["y"] + (1-y)*rect["h"]/2


def assert_3d_pixels(rpc, item, view_id, object_id, logical_width=900):
    """Check a visible object's centre against an empty corner in the live screenshot.

    Call on an unclipped viewport with an unobstructed object and empty top-left corner.
    Inspection/picking alone can pass while the shell fails to composite the app's 3D tile.
    """
    rect = next(r for r in rpc.rects() if r["id"] == view_id)

    def nodes(node):
        yield node
        for child in node.get("children", []):
            yield from nodes(child)

    leaf = next(n for n in nodes(rpc.dump_tree(item)) if n.get("id") == view_id)
    scene = leaf["scene3d"]
    obj = next(o for o in scene["objects"] if o["id"] == object_id)
    x, y = project(scene["camera"], obj["position"], rect)
    assert rect["x"] < x < rect["x"] + rect["w"] and rect["y"] < y < rect["y"] + rect["h"]
    shot = Image(base64.b64decode(rpc.screenshot(item)["png_base64"]))
    scale = shot.width / logical_width
    drawn = shot.at(x * scale, y * scale)
    empty = shot.at((rect["x"] + 4) * scale, (rect["y"] + 4) * scale)
    assert sum(abs(a - b) for a, b in zip(drawn, empty)) > 30, (
        f"no 3D drawn for {view_id}/{object_id}: {drawn} vs empty {empty}"
    )
