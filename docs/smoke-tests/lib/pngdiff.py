"""Changed-pixel counts between two engine screenshots, for the HUD pixel gates.

Usage (as a library):  sys.path.insert(0, "<smoke-tests>/lib"); from pngdiff import load, changed
Usage (as a tool):     python3 pngdiff.py <a.png> <b.png> <x0> <x1> <y0> <y1> [eps]

`load` decodes an 8-bit RGBA PNG with all five scanline filters; `changed` counts the pixels in a
rectangle whose RGB moved by more than `eps` (summed over the channels). The presentation pass
does not preserve alpha, so alpha is ignored. Same-run differencing cancels the world, so a gate
measures only what the HUD drew.
"""
import struct
import sys
import zlib


def load(path):
    data = open(path, "rb").read()
    pos, idat, width, height = 8, b"", 0, 0
    while pos < len(data):
        length = int.from_bytes(data[pos:pos + 4], "big")
        tag = data[pos + 4:pos + 8]
        if tag == b"IHDR":
            width, height, depth, kind = struct.unpack(">IIBB", data[pos + 8:pos + 18])
            assert (depth, kind) == (8, 6), f"expected 8-bit RGBA, got {depth}/{kind}"
        if tag == b"IDAT":
            idat += data[pos + 8:pos + 8 + length]
        pos += 12 + length
    raw = zlib.decompress(idat)
    stride = width * 4
    out = bytearray(height * stride)
    prev = bytearray(stride)

    def paeth(a, b, c):
        p = a + b - c
        pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
        return a if pa <= pb and pa <= pc else (b if pb <= pc else c)

    for y in range(height):
        kind = raw[y * (stride + 1)]
        row = bytearray(raw[y * (stride + 1) + 1:(y + 1) * (stride + 1)])
        if kind == 1:
            for i in range(4, stride):
                row[i] = (row[i] + row[i - 4]) & 0xFF
        elif kind == 2:
            for i in range(stride):
                row[i] = (row[i] + prev[i]) & 0xFF
        elif kind == 3:
            for i in range(stride):
                a = row[i - 4] if i >= 4 else 0
                row[i] = (row[i] + ((a + prev[i]) >> 1)) & 0xFF
        elif kind == 4:
            for i in range(stride):
                a = row[i - 4] if i >= 4 else 0
                c = prev[i - 4] if i >= 4 else 0
                row[i] = (row[i] + paeth(a, prev[i], c)) & 0xFF
        out[y * stride:(y + 1) * stride] = row
        prev = row
    return out, width, height


def changed(a, b, width, x0, x1, y0, y1, eps=60):
    count = 0
    for y in range(y0, y1):
        for x in range(x0, x1):
            i = (y * width + x) * 4
            if abs(a[i] - b[i]) + abs(a[i + 1] - b[i + 1]) + abs(a[i + 2] - b[i + 2]) > eps:
                count += 1
    return count


if __name__ == "__main__":
    first, width, _ = load(sys.argv[1])
    second, _, _ = load(sys.argv[2])
    x0, x1, y0, y1 = map(int, sys.argv[3:7])
    eps = int(sys.argv[7]) if len(sys.argv) > 7 else 60
    print(changed(first, second, width, x0, x1, y0, y1, eps))
