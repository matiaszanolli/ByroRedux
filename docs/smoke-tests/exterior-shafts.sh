#!/usr/bin/env bash
# Exterior sunlight shafts, game-data-free (VLM-SH-001, VLM-SH-002; #6726).
#
# `--shaft-lab` is a low sun behind a row of slats in open sky; `--shaft-lab-solid` is the same scene
# with one unbroken wall, the matched negative control. Both are captured with the in-scatter term
# alone (`--render-debug-mode volumetric_term`) under the engine's default exterior haze floor, in the
# deterministic `renderer-stepped` mode with a pinned camera, so a repeat capture is bit-identical and
# any difference between the two is the shaft and nothing else.
#
# The shaft contrast is (lit - shadow) / (lit + shadow) of the in-scatter term over the wall's body,
# where "lit" is the columns of the gaps and "shadow" the columns of the slats (found from a normal
# frame of the open scene). The gate passes when the open variant's contrast exceeds the solid
# control's by more than twice the run-to-run noise of a repeated open capture (VLM-SH-002), and clears
# two absolute bars so a degenerate pair cannot pass on a noise of zero.
#
# Alpha-tested cutouts (VLM-SH-009) are measured after the shaft gate by three matched captures:
# `--shaft-lab-card` (one panel, the odd stripes cut out by alpha), `--shaft-lab-card-geo` (the same
# stripes as real holes) and `--shaft-lab-card-solid` (one unbroken panel). Their finished frames are
# identical; the in-scatter term over the hole columns differs only if the froxel sun trace stops at a
# cutout. The cutout must come within 2 % of the geometry holes. That is KNOWN RED today (the trace is
# opaque, PRD section 3 row 17), so it is reported and does not fail the run unless BYRO_SHAFT_CARD_GATE=1.
#
# Usage: docs/smoke-tests/exterior-shafts.sh
# BYRO_SHAFT_BIN overrides the engine binary; BYRO_SHAFT_OUT keeps captures in a chosen directory.
# An X display or xvfb-run, a Vulkan device and python3 with numpy and Pillow are needed.
# Exit 77 (SKIP, never a pass) when one of those is missing.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
BIN="${BYRO_SHAFT_BIN:-}"
if [[ -z "$BIN" ]]; then
    for candidate in "$ROOT/target/release/byroredux" "$ROOT/target/debug/byroredux"; do
        [[ -x "$candidate" ]] && { BIN="$candidate"; break; }
    done
fi
OUT="${BYRO_SHAFT_OUT:-$(mktemp -d /tmp/byro-shaft-smoke.XXXXXX)}"

if [[ -z "$BIN" || ! -x "$BIN" ]]; then
    echo "SKIP: engine binary not found; build byroredux first" >&2
    exit 77
fi
if ! python3 -c 'import numpy, PIL' 2>/dev/null; then
    echo "SKIP: python3 with numpy and Pillow is needed to measure the captures" >&2
    exit 77
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

capture() { # <name> <lab flag> <render debug view>
    local name="$1" lab="$2" view="$3"
    (cd "$OUT" && env -u WAYLAND_DISPLAY -u BYRO_HAZE_FLOOR XDG_SESSION_TYPE=x11 \
        BYROREDUX_SETTINGS_PATH="$OUT/settings_$name.toml" RUST_LOG=warn \
        "${DISPLAY_RUNNER[@]}" timeout 180s "$BIN" \
        "$lab" --fly --window-size 1280x720 --upscaler taa \
        --bench-mode renderer-stepped --bench-camera static --bench-frames 90 \
        --render-debug-mode "$view" --screenshot "$name.png" >"$OUT/$name.log" 2>&1) || {
        echo "FAIL: capture $name failed; see $OUT/$name.log" >&2
        tail -20 "$OUT/$name.log" >&2
        exit 1
    }
    [[ -s "$OUT/$name.png" ]] || { echo "FAIL: missing $OUT/$name.png" >&2; exit 1; }
}

capture open_final --shaft-lab final
capture open_vol --shaft-lab volumetric_term
capture open_vol_repeat --shaft-lab volumetric_term
capture solid_vol --shaft-lab-solid volumetric_term
capture card_vol --shaft-lab-card volumetric_term
capture cardgeo_final --shaft-lab-card-geo final
capture cardgeo_vol --shaft-lab-card-geo volumetric_term
capture cardsolid_vol --shaft-lab-card-solid volumetric_term

python3 - "$OUT" <<'PY'
import sys
from pathlib import Path

import numpy as np
from PIL import Image

out = Path(sys.argv[1])


def load(name):
    return np.asarray(Image.open(out / f"{name}.png").convert("L"), dtype=np.float32)


# The wall's body in the 1280x720 frame: below the lintel, above the ground line.
ROWS, COLS = slice(335, 520), slice(375, 905)

final = load("open_final")[ROWS, COLS]
column_mean = final.mean(axis=0)
slat = column_mean < (column_mean.min() + column_mean.max()) / 2
if slat.sum() < 20 or (~slat).sum() < 20:
    print(f"FAIL: could not tell slats from gaps ({int(slat.sum())} / {int((~slat).sum())} columns)")
    sys.exit(1)


def contrast(name):
    band = load(name)[ROWS, COLS]
    lit, shadow = band[:, ~slat].mean(), band[:, slat].mean()
    return (lit - shadow) / (lit + shadow + 1e-6), lit, shadow


open_c, open_lit, open_shadow = contrast("open_vol")
repeat_c, _, _ = contrast("open_vol_repeat")
solid_c, solid_lit, solid_shadow = contrast("solid_vol")
noise = abs(open_c - repeat_c)

print(f"open  : lit {open_lit:.1f} shadow {open_shadow:.1f} contrast {open_c:.4f}")
print(f"solid : lit {solid_lit:.1f} shadow {solid_shadow:.1f} contrast {solid_c:.4f}")
print(f"repeat noise (|open - open repeat|): {noise:.6f}")

failures = []
if not open_c - solid_c > 2.0 * noise:
    failures.append("VLM-SH-002: open contrast does not exceed the solid control by more than twice the noise")
if open_c < 0.05:
    failures.append(f"open contrast {open_c:.4f} is below the 0.05 bar: the sun does not light the gaps")
if solid_c > 0.02:
    failures.append(f"solid contrast {solid_c:.4f} is above the 0.02 bar: the control shows a shaft")
if failures:
    for line in failures:
        print("FAIL:", line)
    sys.exit(1)
print("PASS: exterior shafts")
PY

BYRO_SHAFT_CARD_GATE="${BYRO_SHAFT_CARD_GATE:-0}" python3 - "$OUT" <<'PY'
import os
import sys
from pathlib import Path

import numpy as np
from PIL import Image

out = Path(sys.argv[1])


def load(name):
    return np.asarray(Image.open(out / f"{name}.png").convert("L"), dtype=np.float32)


# The card's body in the 1280x720 frame, and its hole columns (from the geometry variant's finished frame).
ROWS, COLS = slice(330, 520), slice(520, 760)
final = load("cardgeo_final")[ROWS, COLS]
column_mean = final.mean(axis=0)
stripe = column_mean < (column_mean.min() + column_mean.max()) / 2
if stripe.sum() < 20 or (~stripe).sum() < 20:
    print(f"FAIL: could not tell the card's stripes from its holes ({int(stripe.sum())} / {int((~stripe).sum())} columns)")
    sys.exit(1)


def holes(name):
    return load(name)[ROWS, COLS][:, ~stripe].mean()


cutout, geometry, solid = holes("card_vol"), holes("cardgeo_vol"), holes("cardsolid_vol")
print(f"card holes: cutout {cutout:.2f}  geometry {geometry:.2f}  solid {solid:.2f}")
if not geometry - solid > 1.0:
    print("FAIL: the geometry holes show no shaft over the solid panel, so the card cannot be judged")
    sys.exit(1)
gap = (geometry - cutout) / geometry
print(f"VLM-SH-009 cutout-vs-hole gap {gap * 100:.1f} % (bar 2 %)")
if abs(gap) <= 0.02:
    print("PASS: the froxel sun trace honours alpha coverage")
elif os.environ["BYRO_SHAFT_CARD_GATE"] == "1":
    print("FAIL: VLM-SH-009: the froxel sun trace does not honour alpha coverage")
    sys.exit(1)
else:
    print("KNOWN RED: VLM-SH-009 (#6726): the froxel sun trace does not honour alpha coverage")
PY
echo "captures and logs kept in $OUT"
