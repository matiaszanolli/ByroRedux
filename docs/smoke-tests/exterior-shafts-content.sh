#!/usr/bin/env bash
# In-scatter radiance at noon and two low-sun hours on the five exterior fixtures (VLM-SH-006, A8; #6773).
#
# For each of FNV, FO3, Oblivion, Skyrim and FO4 (the `m-exteriors.sh` fixture cells, default spawn pose, the
# deterministic `renderer-stepped` static camera) it captures the finished frame and the froxel in-scatter
# radiance view (`--render-debug-mode volumetric_radiance`, exponentially mapped, no opacity floor) at 12:00,
# 07:00 and 17:30 (`BYRO_HOUR`), then prints the frame's mean, 95th and 5th percentile in-scatter radiance
# (the mapped view inverted to linear) and the finished frame's mean. It MEASURES and exits 0.
#
# What it does not give is a shaft contrast across a named occluder: that needs a lit and an occluded sample
# at the same depth, and these poses offer none (a pole or trunk column reads darker because its pixels sit
# nearer, so less air has accumulated, not because the air is shadowed; see the fog design's content notes).
#
# Usage: docs/smoke-tests/exterior-shafts-content.sh [fnv|fo3|obl|sky|fo4|all]
# BYRO_CONTENT_BIN overrides the engine binary; BYRO_CONTENT_OUT keeps captures; BYROREDUX_GAMES_DIR is the Steam
# `common` directory. Needs a display or `xvfb-run`, a Vulkan device, python3 with numpy and Pillow. Exit 77 to skip.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
BIN="${BYRO_CONTENT_BIN:-$ROOT/target/release/byroredux}"
OUT="${BYRO_CONTENT_OUT:-$(mktemp -d /tmp/byro-shaft-content.XXXXXX)}"
GAMES="${BYROREDUX_GAMES_DIR:-/mnt/data/SteamLibrary/steamapps/common}"
WHICH="${1:-all}"
HOURS=(12 7 17.5)

[[ -x "$BIN" ]] || { echo "SKIP: engine binary not found at $BIN" >&2; exit 77; }
mkdir -p "$OUT"
if [[ -z "${DISPLAY:-}" ]]; then
    command -v xvfb-run >/dev/null || { echo "SKIP: no X display or xvfb-run" >&2; exit 77; }
    RUNNER=(xvfb-run -a)
else
    RUNNER=()
fi

# fixture <game> -> sets DATA_DIR and ARGS for the fixture cell
fixture() {
    case "$1" in
    fnv) DATA_DIR="$GAMES/Fallout New Vegas/Data"
         ARGS=(--esm "$DATA_DIR/FalloutNV.esm" --grid 0,0 --radius 1 --wrld WastelandNV
               --bsa "$DATA_DIR/Fallout - Meshes.bsa" --bsa "$DATA_DIR/Update.bsa"
               --textures-bsa "$DATA_DIR/Fallout - Textures.bsa" --textures-bsa "$DATA_DIR/Update.bsa") ;;
    fo3) DATA_DIR="$GAMES/Fallout 3 goty/Data"
         ARGS=(--esm "$DATA_DIR/Fallout3.esm" --grid -1,-7 --radius 1 --wrld MegatonWorld
               --bsa "$DATA_DIR/Fallout - Meshes.bsa" --textures-bsa "$DATA_DIR/Fallout - Textures.bsa") ;;
    obl) DATA_DIR="$GAMES/Oblivion/Data"
         ARGS=(--esm "$DATA_DIR/Oblivion.esm" --grid 0,0 --radius 1 --wrld Tamriel
               --bsa "$DATA_DIR/Oblivion - Meshes.bsa" --textures-bsa "$DATA_DIR/Oblivion - Textures - Compressed.bsa") ;;
    sky) DATA_DIR="$GAMES/Skyrim Special Edition/Data"
         ARGS=(--esm "$DATA_DIR/Skyrim.esm" --grid 2,-4 --radius 1 --wrld Tamriel
               --bsa "$DATA_DIR/Skyrim - Meshes0.bsa" --bsa "$DATA_DIR/Skyrim - Meshes1.bsa")
         for i in 0 1 2 3 4 5 6 7 8; do ARGS+=(--textures-bsa "$DATA_DIR/Skyrim - Textures$i.bsa"); done ;;
    fo4) DATA_DIR="$GAMES/Fallout 4/Data"
         ARGS=(--esm "$DATA_DIR/Fallout4.esm" --grid 0,0 --radius 1 --wrld Commonwealth
               --bsa "$DATA_DIR/Fallout4 - Meshes.ba2" --bsa "$DATA_DIR/Fallout4 - MeshesExtra.ba2")
         for i in 1 2 3 4 5 6 7 8 9; do ARGS+=(--textures-bsa "$DATA_DIR/Fallout4 - Textures$i.ba2"); done
         ARGS+=(--textures-bsa "$DATA_DIR/Fallout4 - TexturesPatch.ba2" --materials-ba2 "$DATA_DIR/Fallout4 - Materials.ba2") ;;
    esac
    [[ -d "$DATA_DIR" ]]
}

capture() { # game hour kind(final|radiance)
    local game="$1" hour="$2" kind="$3" debug=()
    [[ "$kind" == radiance ]] && debug=(--render-debug-mode volumetric_radiance)
    ( cd "$DATA_DIR" && env -u WAYLAND_DISPLAY XDG_SESSION_TYPE=x11 BYRO_DEBUG_SERVER=0 BYRO_HOUR="$hour" \
        "${RUNNER[@]}" timeout 300s "$BIN" "${ARGS[@]}" --bench-mode renderer-stepped --bench-camera static \
        --bench-frames 40 --upscaler taa --window-size 1280x720 "${debug[@]}" \
        --screenshot "$OUT/${game}_${hour}_${kind}.png" ) >"$OUT/${game}_${hour}_${kind}.log" 2>&1 \
        || { echo "FAIL: $game $hour $kind; see $OUT/${game}_${hour}_${kind}.log" >&2; exit 1; }
}

games=(fnv fo3 obl sky fo4)
[[ "$WHICH" != all ]] && games=("$WHICH")
ran=()
for game in "${games[@]}"; do
    if ! fixture "$game"; then echo "SKIP $game: data not found" >&2; continue; fi
    for hour in "${HOURS[@]}"; do
        capture "$game" "$hour" radiance
        capture "$game" "$hour" final
    done
    ran+=("$game")
done

python3 -I - "$OUT" "${ran[@]}" <<'PY'
import sys
from pathlib import Path

import numpy as np
from PIL import Image

out, games = Path(sys.argv[1]), sys.argv[2:]
print("in-scatter radiance, linear (mapped view inverted): frame mean / p95 / p5; finished-frame mean")
for game in games:
    cells = []
    for hour in ("12", "7", "17.5"):
        rad = np.asarray(Image.open(out / f"{game}_{hour}_radiance.png").convert("RGB")).astype(float) / 255
        x = -np.log(1 - np.clip(rad.mean(axis=2), 0, 0.999))
        fin = np.asarray(Image.open(out / f"{game}_{hour}_final.png").convert("RGB")).astype(float).mean()
        cells.append(f"{hour:>4}h {x.mean():.4f} / {np.percentile(x, 95):.4f} / {np.percentile(x, 5):.4f}; {fin:6.1f}")
    print(f"{game:4s} " + " | ".join(cells))
print(f"Captures and logs: {out}")
PY
