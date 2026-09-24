#!/usr/bin/env bash
# Compare a controlled interior with an unmarked roof opening against the
# same room with a sealed roof. The localized pixel difference must show a
# shaft, rather than a global exposure or fog change. No game data needed.
#
# Usage: docs/smoke-tests/interior-godrays.sh
# BYRO_GODRAY_BIN overrides the engine binary; BYRO_GODRAY_OUT keeps captures
# in a chosen directory. An X display or xvfb-run and a Vulkan device are needed.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
BIN="${BYRO_GODRAY_BIN:-$ROOT/target/debug/byroredux}"
OUT="${BYRO_GODRAY_OUT:-$(mktemp -d /tmp/byro-godray-smoke.XXXXXX)}"

if [[ ! -x "$BIN" ]]; then
    echo "FAIL: engine binary not found at $BIN; build byroredux first" >&2
    exit 1
fi

mkdir -p "$OUT"
if [[ -z "${DISPLAY:-}" ]]; then
    if ! command -v xvfb-run >/dev/null; then
        echo "SKIP: no X display or xvfb-run" >&2
        exit 77
    fi
    DISPLAY_RUNNER=(xvfb-run -a)
else
    DISPLAY_RUNNER=()
fi

for mode in open sealed; do
    if [[ "$mode" == open ]]; then
        scene=--godray-lab
    else
        scene=--godray-lab-sealed
    fi
    env -u WAYLAND_DISPLAY XDG_SESSION_TYPE=x11 \
        "${DISPLAY_RUNNER[@]}" timeout 90s "$BIN" \
        "$scene" --fly --window-size 320x180 --bench-frames 1 \
        --screenshot "$OUT/$mode.png" >"$OUT/$mode.log" 2>&1 || {
            echo "FAIL: $mode capture failed; see $OUT/$mode.log" >&2
            tail -20 "$OUT/$mode.log" >&2
            exit 1
        }
    if [[ ! -s "$OUT/$mode.png" ]]; then
        echo "FAIL: missing $OUT/$mode.png" >&2
        exit 1
    fi
done

python3 - "$OUT" <<'PY'
import struct
import sys
import zlib
from pathlib import Path


def rgba(path):
    data = path.read_bytes()
    assert data[:8] == b"\x89PNG\r\n\x1a\n", f"not PNG: {path}"
    pos, compressed = 8, bytearray()
    width = height = 0
    while pos < len(data):
        size = int.from_bytes(data[pos:pos + 4], "big")
        kind = data[pos + 4:pos + 8]
        payload = data[pos + 8:pos + 8 + size]
        if kind == b"IHDR":
            width, height, depth, color = struct.unpack(">IIBB", payload[:10])
            assert (depth, color) == (8, 6), f"expected 8-bit RGBA: {path}"
        elif kind == b"IDAT":
            compressed.extend(payload)
        pos += size + 12

    raw = zlib.decompress(compressed)
    stride = width * 4
    pixels = bytearray(height * stride)
    previous = bytearray(stride)

    def paeth(a, b, c):
        estimate = a + b - c
        distances = (abs(estimate - a), abs(estimate - b), abs(estimate - c))
        return (a, b, c)[distances.index(min(distances))]

    for y in range(height):
        filter_type = raw[y * (stride + 1)]
        row = bytearray(raw[y * (stride + 1) + 1:(y + 1) * (stride + 1)])
        assert filter_type in range(5), f"unknown PNG filter {filter_type}"
        for x in range(stride):
            left = row[x - 4] if x >= 4 else 0
            above = previous[x]
            upper_left = previous[x - 4] if x >= 4 else 0
            if filter_type == 1:
                predictor = left
            elif filter_type == 2:
                predictor = above
            elif filter_type == 3:
                predictor = (left + above) // 2
            elif filter_type == 4:
                predictor = paeth(left, above, upper_left)
            else:
                predictor = 0
            row[x] = (row[x] + predictor) & 0xff
        pixels[y * stride:(y + 1) * stride] = row
        previous = row
    return pixels, width, height


out = Path(sys.argv[1])
open_pixels, width, height = rgba(out / "open.png")
sealed_pixels, other_width, other_height = rgba(out / "sealed.png")
assert (width, height) == (other_width, other_height) == (320, 180)


def mean_delta(x0, x1, y0=65, y1=175):
    total = 0
    for y in range(y0, y1):
        for x in range(x0, x1):
            offset = (y * width + x) * 4
            total += sum(open_pixels[offset + c] - sealed_pixels[offset + c]
                         for c in range(3)) / 3
    return total / ((x1 - x0) * (y1 - y0))


center = mean_delta(135, 185)
side = mean_delta(45, 95)
assert center > 25, f"FAIL: no bright shaft through opening (center Δ={center:.1f})"
assert center > side + 15, (
    f"FAIL: opening does not localize illumination (center Δ={center:.1f}, "
    f"side Δ={side:.1f})"
)


def mean_rgb(pixels, x0, x1, y0, y1):
    total = 0
    for y in range(y0, y1):
        for x in range(x0, x1):
            offset = (y * width + x) * 4
            total += sum(pixels[offset + c] for c in range(3)) / 3
    return total / ((x1 - x0) * (y1 - y0))


open_sky = mean_rgb(open_pixels, 150, 170, 8, 28)
sealed_sky = mean_rgb(sealed_pixels, 150, 170, 8, 28)
roof = mean_rgb(open_pixels, 70, 90, 8, 28)
assert open_sky > sealed_sky + 80, (
    f"FAIL: no outdoor sky through opening ({open_sky:.1f} vs {sealed_sky:.1f})"
)
assert open_sky > roof + 80, (
    f"FAIL: sky is not confined to the opening ({open_sky:.1f} vs {roof:.1f})"
)
print(f"PASS: interior shaft center Δ={center:.1f}, side Δ={side:.1f} RGB levels")
print(f"PASS: outdoor sky {open_sky:.1f} through opening, {roof:.1f} on roof")
print(f"Captures and logs: {out}")
PY
