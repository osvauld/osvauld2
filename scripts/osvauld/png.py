"""Read pixels out of a bridge screenshot without Pillow. RGBA, 8-bit, non-interlaced — what
`Screenshot` produces, and nothing else."""

import struct
import zlib


class Image:
    def __init__(self, png: bytes):
        pos, idat = 8, b""
        while pos < len(png):
            (size,) = struct.unpack(">I", png[pos : pos + 4])
            kind = png[pos + 4 : pos + 8]
            if kind == b"IHDR":
                self.width, self.height, depth, colour = struct.unpack(">IIBB", png[pos + 8 : pos + 18])
                if (depth, colour) != (8, 6):
                    raise ValueError(f"expected 8-bit RGBA, got depth {depth} colour type {colour}")
            elif kind == b"IDAT":
                idat += png[pos + 8 : pos + 8 + size]
            pos += 12 + size
        raw, stride = zlib.decompress(idat), self.width * 4
        rows, previous, cursor = [], bytearray(stride), 0
        for _ in range(self.height):
            kind, cursor = raw[cursor], cursor + 1
            line = bytearray(raw[cursor : cursor + stride])
            cursor += stride
            for i in range(stride):
                left = line[i - 4] if i >= 4 else 0
                up = previous[i]
                up_left = previous[i - 4] if i >= 4 else 0
                if kind == 1:
                    line[i] = (line[i] + left) & 255
                elif kind == 2:
                    line[i] = (line[i] + up) & 255
                elif kind == 3:
                    line[i] = (line[i] + (left + up) // 2) & 255
                elif kind == 4:
                    guess = left + up - up_left
                    dl, du, dul = abs(guess - left), abs(guess - up), abs(guess - up_left)
                    line[i] = (line[i] + (left if dl <= du and dl <= dul else up if du <= dul else up_left)) & 255
            rows.append(bytes(line))
            previous = line
        self.rows = rows

    def at(self, x: float, y: float) -> tuple[int, int, int]:
        """The pixel at a *physical* coordinate."""
        row, i = self.rows[int(y)], int(x) * 4
        return row[i], row[i + 1], row[i + 2]


def near(rgb: tuple[int, int, int], hex_colour: str, tolerance: int = 24) -> bool:
    want = tuple(int(hex_colour[i : i + 2], 16) for i in (1, 3, 5))
    return all(abs(a - b) <= tolerance for a, b in zip(rgb, want))
