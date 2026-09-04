#!/usr/bin/env python3
"""Compare 8-bit RGB/RGBA PNG captures without third-party dependencies."""

from __future__ import annotations

import argparse
import struct
import sys
import zlib
from pathlib import Path


def paeth(left: int, above: int, upper_left: int) -> int:
    estimate = left + above - upper_left
    left_distance = abs(estimate - left)
    above_distance = abs(estimate - above)
    upper_left_distance = abs(estimate - upper_left)
    if left_distance <= above_distance and left_distance <= upper_left_distance:
        return left
    if above_distance <= upper_left_distance:
        return above
    return upper_left


def decode_png(path: Path) -> tuple[int, int, bytes]:
    data = path.read_bytes()
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError(f"{path}: not a PNG")

    offset = 8
    width = height = channels = 0
    compressed = bytearray()
    while offset < len(data):
        length = struct.unpack_from(">I", data, offset)[0]
        kind = data[offset + 4 : offset + 8]
        payload = data[offset + 8 : offset + 8 + length]
        offset += 12 + length
        if kind == b"IHDR":
            width, height, depth, color_type, compression, filtering, interlace = (
                struct.unpack(">IIBBBBB", payload)
            )
            if depth != 8 or color_type not in (2, 6):
                raise ValueError(
                    f"{path}: expected 8-bit RGB/RGBA, got depth={depth} type={color_type}"
                )
            if compression != 0 or filtering != 0 or interlace != 0:
                raise ValueError(f"{path}: unsupported compressed/interlaced PNG layout")
            channels = 3 if color_type == 2 else 4
        elif kind == b"IDAT":
            compressed.extend(payload)
        elif kind == b"IEND":
            break

    if width == 0 or height == 0 or channels == 0:
        raise ValueError(f"{path}: missing IHDR")
    raw = zlib.decompress(compressed)
    stride = width * channels
    expected = height * (stride + 1)
    if len(raw) != expected:
        raise ValueError(f"{path}: decoded {len(raw)} bytes, expected {expected}")

    previous = bytearray(stride)
    rgb = bytearray(width * height * 3)
    raw_offset = 0
    rgb_offset = 0
    for _ in range(height):
        filter_kind = raw[raw_offset]
        source = raw[raw_offset + 1 : raw_offset + 1 + stride]
        raw_offset += stride + 1
        row = bytearray(stride)
        for index, value in enumerate(source):
            left = row[index - channels] if index >= channels else 0
            above = previous[index]
            upper_left = previous[index - channels] if index >= channels else 0
            if filter_kind == 0:
                predictor = 0
            elif filter_kind == 1:
                predictor = left
            elif filter_kind == 2:
                predictor = above
            elif filter_kind == 3:
                predictor = (left + above) // 2
            elif filter_kind == 4:
                predictor = paeth(left, above, upper_left)
            else:
                raise ValueError(f"{path}: unsupported PNG filter {filter_kind}")
            row[index] = (value + predictor) & 0xFF
        for pixel in range(width):
            source_offset = pixel * channels
            rgb[rgb_offset : rgb_offset + 3] = row[source_offset : source_offset + 3]
            rgb_offset += 3
        previous = row
    return width, height, bytes(rgb)


def compare(reference: bytes, candidate: bytes, channel_tolerance: int) -> tuple[float, float, int]:
    changed_pixels = 0
    absolute_sum = 0
    maximum = 0
    for offset in range(0, len(reference), 3):
        pixel_changed = False
        for channel in range(3):
            delta = abs(reference[offset + channel] - candidate[offset + channel])
            absolute_sum += delta
            maximum = max(maximum, delta)
            pixel_changed |= delta > channel_tolerance
        changed_pixels += int(pixel_changed)
    pixels = len(reference) // 3
    return changed_pixels / pixels, absolute_sum / len(reference), maximum


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--channel-tolerance", type=int, default=2)
    parser.add_argument("--max-changed-fraction", type=float, required=True)
    parser.add_argument("--max-mean-absolute-error", type=float, required=True)
    parser.add_argument("images", nargs="+", type=Path)
    args = parser.parse_args()
    if len(args.images) < 2:
        parser.error("at least two images are required")

    width, height, reference = decode_png(args.images[0])
    failed = False
    for candidate_path in args.images[1:]:
        candidate_width, candidate_height, candidate = decode_png(candidate_path)
        if (candidate_width, candidate_height) != (width, height):
            raise ValueError(
                f"{candidate_path}: dimensions {candidate_width}x{candidate_height} "
                f"do not match {width}x{height}"
            )
        changed, mean_error, maximum = compare(
            reference, candidate, args.channel_tolerance
        )
        print(
            f"{candidate_path}: changed={changed:.6f} mean_abs={mean_error:.6f} "
            f"max={maximum}"
        )
        failed |= changed > args.max_changed_fraction
        failed |= mean_error > args.max_mean_absolute_error
    return 1 if failed else 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError, zlib.error) as error:
        print(f"png-stability: {error}", file=sys.stderr)
        raise SystemExit(2) from error
