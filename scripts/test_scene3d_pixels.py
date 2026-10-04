"""The pixel gate must reject a metadata-rich scene when the screenshot is empty."""
import math
import unittest
from unittest.mock import patch

from osvauld.scene3d import assert_3d_pixels


class Rpc:
    def rects(self):
        return [{"id": "view", "x": 20, "y": 20, "w": 80, "h": 80}]

    def dump_tree(self, item):
        return {"id": "view", "scene3d": {
            "camera": {"eye": [0,0,5], "target": [0,0,0], "up": [0,1,0],
                       "fov_y_radians": math.pi / 2},
            "objects": [{"id": "ball", "position": [0,0,0]}],
        }}

    def screenshot(self, item):
        return {"png_base64": ""}


class Pixels:
    width = 1800

    def __init__(self, _, drawn=False):
        self.drawn = drawn

    def at(self, x, y):
        return (99, 99, 99) if self.drawn and (x, y) == (120, 120) else (8, 8, 8)


class PixelGate(unittest.TestCase):
    def test_inspection_without_pixels_fails(self):
        with patch("osvauld.scene3d.Image", Pixels):
            with self.assertRaisesRegex(AssertionError, "no 3D drawn"):
                assert_3d_pixels(Rpc(), "item", "view", "ball")

    def test_projected_physical_pixel_passes(self):
        with patch("osvauld.scene3d.Image", lambda data: Pixels(data, drawn=True)):
            assert_3d_pixels(Rpc(), "item", "view", "ball")


if __name__ == "__main__":
    unittest.main()
