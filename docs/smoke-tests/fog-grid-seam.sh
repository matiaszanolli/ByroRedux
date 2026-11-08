#!/usr/bin/env bash
# Grid-reach seam baseline (VLM-HF-007, scenario A6; #6724).
#
# Pins a camera on an FNV and a Skyrim exterior and captures the same frame at the default
# `--fog-grid-far-m` (twice, for the noise floor) and at the larger value. VLM-HF-007 requires the
# distant pixels to keep their mean luminance when the grid's reach changes. A repeat in this mode is
# deterministic (noise ~0), so the literal bar "no more than the repeat's noise" cannot hold where the grid
# resolves real shadows beyond its default reach; the gate is a tolerance, SEAM_TOLERANCE levels (default
# 2.5 of 255), above the repeat noise. Before the tail shared the grid's height profile and mean density
# the same captures moved 4 to 14 levels (PRD revisions 17 and 24); now they move at most about 2.
# GATE=0 turns the script back into a measurement that always exits 0.
#
# Both captures use `--bench-mode renderer-stepped --bench-camera static`, where a repeat is
# deterministic. On the FNV pose that mode renders the 3D scene rolled 180 degrees (the HUD stays
# upright; `--bench-mode system-live` renders the same pose upright, and the Skyrim pose is upright in
# both), so the analysis rotates only the FNV frames back before reading their regions.
#
# Usage: docs/smoke-tests/fog-grid-seam.sh [fnv|skyrim|all]
# BYRO_SEAM_BIN overrides the engine binary; BYRO_SEAM_OUT keeps captures in a chosen directory;
# BYROREDUX_FNV_DATA / BYROREDUX_SKYRIM_DATA name the Data directories (defaults below);
# SEAM_TOLERANCE sets the gate (default 2.5). Exit 77 when data or a display is missing.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
BIN="${BYRO_SEAM_BIN:-$ROOT/target/release/byroredux}"
OUT="${BYRO_SEAM_OUT:-$(mktemp -d /tmp/byro-fog-seam.XXXXXX)}"
WHICH="${1:-all}"
FNV_DATA="${BYROREDUX_FNV_DATA:-/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data}"
SKYRIM_DATA="${BYROREDUX_SKYRIM_DATA:-/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data}"
NEAR_M=128
FAR_M=512

[[ -x "$BIN" ]] || { echo "SKIP: engine binary not found at $BIN" >&2; exit 77; }
mkdir -p "$OUT"
if [[ -z "${DISPLAY:-}" ]]; then
    command -v xvfb-run >/dev/null || { echo "SKIP: no X display or xvfb-run" >&2; exit 77; }
    RUNNER=(xvfb-run -a)
else
    RUNNER=()
fi

# capture <label> <data dir> <far m> <tag> <engine args...>
capture() {
    local label="$1" data="$2" far="$3" tag="$4"
    shift 4
    ( cd "$data" && env -u WAYLAND_DISPLAY XDG_SESSION_TYPE=x11 BYRO_DEBUG_SERVER=0 \
        "${RUNNER[@]}" timeout 400s "$BIN" "$@" \
        --bench-mode renderer-stepped --fly --bench-camera static --bench-frames 40 \
        --upscaler taa --window-size 1280x720 --fog-grid-far-m "$far" \
        --screenshot "$OUT/${label}_${tag}.png" ) >"$OUT/${label}_${tag}.log" 2>&1 || {
        echo "FAIL: $label $tag capture failed; see $OUT/${label}_${tag}.log" >&2
        tail -15 "$OUT/${label}_${tag}.log" >&2
        exit 1
    }
}

run_fnv() {
    [[ -f "$FNV_DATA/FalloutNV.esm" ]] || { echo "SKIP fnv: no FalloutNV.esm in $FNV_DATA" >&2; return 0; }
    # Goodsprings, the Prospector Saloon door's arrival pose (probe_slice_fixture), turned to face the town.
    local args=(--esm "$FNV_DATA/FalloutNV.esm" --grid -17,0 --radius 3 --wrld WastelandNV
        --bsa "$FNV_DATA/Fallout - Meshes.bsa" --bsa "$FNV_DATA/Update.bsa"
        --textures-bsa "$FNV_DATA/Fallout - Textures.bsa" --textures-bsa "$FNV_DATA/Update.bsa"
        --camera-pos -67763,8516,-3567 --camera-forward 0.0,0.03,1)
    capture fnv "$FNV_DATA" "$NEAR_M" a "${args[@]}"
    capture fnv "$FNV_DATA" "$NEAR_M" b "${args[@]}"
    capture fnv "$FNV_DATA" "$FAR_M" far "${args[@]}"
}

run_skyrim() {
    [[ -f "$SKYRIM_DATA/Skyrim.esm" ]] || { echo "SKIP skyrim: no Skyrim.esm in $SKYRIM_DATA" >&2; return 0; }
    # An elevated pose over the Whiterun plains (grid 2,-4), looking north-east at Dragonsreach and the far ridges.
    local args=(--esm "$SKYRIM_DATA/Skyrim.esm" --grid 2,-4 --radius 3 --wrld Tamriel
        --bsa "$SKYRIM_DATA/Skyrim - Meshes0.bsa" --bsa "$SKYRIM_DATA/Skyrim - Meshes1.bsa"
        --camera-pos 10240,8000,14336 --camera-forward 0.5,0.0,-1)
    for i in 0 1 2 3 4 5 6 7 8; do args+=(--textures-bsa "$SKYRIM_DATA/Skyrim - Textures$i.bsa"); done
    capture skyrim "$SKYRIM_DATA" "$NEAR_M" a "${args[@]}"
    capture skyrim "$SKYRIM_DATA" "$NEAR_M" b "${args[@]}"
    capture skyrim "$SKYRIM_DATA" "$FAR_M" far "${args[@]}"
}

case "$WHICH" in
    fnv) run_fnv ;;
    skyrim) run_skyrim ;;
    all) run_fnv; run_skyrim ;;
    *) echo "usage: $0 [fnv|skyrim|all]" >&2; exit 2 ;;
esac

python3 -I - "$OUT" "$NEAR_M" "$FAR_M" "${GATE:-1}" "${SEAM_TOLERANCE:-2.5}" <<'PY'
import sys
from pathlib import Path

import numpy as np
from PIL import Image

out, near, far, gate, tolerance = Path(sys.argv[1]), sys.argv[2], sys.argv[3], sys.argv[4] == "1", float(sys.argv[5])

# Row / column windows (720p, after undoing the roll) over content that sits well beyond the
# near plane in each pose. They approximate "pixels beyond the larger far plane": no depth view
# exists, so these are the distant hills, ridges, forest and sky of the named pose.
REGIONS = {
    "fnv": {"whole": (0, 720, 0, 1280), "hills L": (330, 400, 130, 620),
            "hills R": (210, 380, 830, 1280), "sky band": (170, 330, 420, 1100)},
    "skyrim": {"whole": (0, 720, 0, 1280), "terrain haze": (360, 720, 0, 1280),
               "far ridge": (350, 420, 700, 1100), "forest L": (420, 540, 0, 260),
               "sky": (0, 330, 0, 1280)},
}


ROLLED = {"fnv"}  # frames the stepped bench renders rolled 180 degrees


def load(label, tag):
    img = np.asarray(Image.open(out / f"{label}_{tag}.png").convert("RGB")).astype(float)
    return (np.rot90(img, 2) if label in ROLLED else img).mean(axis=2)


worst = 0.0
for label, regions in REGIONS.items():
    if not (out / f"{label}_far.png").exists():
        continue
    a, b, c = load(label, "a"), load(label, "b"), load(label, "far")
    print(f"{label}: repeat max |dL| {np.abs(a - b).max():.1f}, mean {np.abs(a - b).mean():.4f}")
    for name, (r0, r1, c0, c1) in regions.items():
        m = lambda x: x[r0:r1, c0:c1].mean()
        noise = abs(m(a) - m(b))
        delta = m(c) - (m(a) + m(b)) / 2
        worst = max(worst, abs(delta) - noise)
        print(f"  {name:13s} grid {near} m: {m(a):7.3f} / {m(b):7.3f} (noise {noise:5.3f})   "
              f"grid {far} m: {m(c):7.3f}   delta {delta:+7.3f}")
print(f"Captures and logs: {out}")
print(f"worst distant-region delta past the repeat noise: {worst:.2f} levels (tolerance {tolerance})")
if gate and worst > tolerance:
    sys.exit(f"FAIL: a distant region moved {worst:.2f} levels (tolerance {tolerance}) when the grid reach changed")
PY
