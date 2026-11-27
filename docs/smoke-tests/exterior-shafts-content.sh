#!/usr/bin/env bash
# Shaft contrast and in-scatter radiance on the five exterior fixtures (VLM-SH-006, -012, A8; #6773).
#
# For each of FNV, FO3, Oblivion, Skyrim and FO4 (the `m-exteriors.sh` fixture cells) it puts the deterministic
# `renderer-stepped` static camera at the view the owner named for VLM-SH-012 and captures the finished frame and
# the froxel in-scatter radiance view (`--render-debug-mode volumetric_radiance`, exponentially mapped, no opacity
# floor) at 12:00 and the view's low-sun hour, each with the froxel sun shadow on and off
# (`BYRO_FROXEL_SUN_OCCLUSION`, which this script sets), then repeats the low-sun radiance pair: 10 captures a game.
#
# Shaft contrast (PRD §2) is 1 - mean(shadow on) / mean(shadow off) of linear radiance over the view's fixed sample
# region. The run FAILS (exit 1) unless every view meets the content contrast rule (PRD §10 Q15): at its low-sun
# hour the contrast is at least 0.05 and more than twice |value - repeat value|. Noon is printed, not judged.
#
# Each region was chosen at a strong part of its shaft, so a pass says the froxel sun shadow still acts there. It
# does not say the shaft shows in the finished frame; Oblivion's does not. Never re-select a region from the captures
# being measured. How the views were found, and what limits each: the fog design's "Content shaft pose scouting".
#
# Usage: docs/smoke-tests/exterior-shafts-content.sh [fnv|fo3|obl|sky|fo4|all]
# BYRO_CONTENT_BIN overrides the engine binary; BYRO_CONTENT_OUT keeps captures; BYROREDUX_GAMES_DIR is the Steam
# `common` directory. Needs a display or `xvfb-run`, a Vulkan device, python3 with numpy and Pillow. Exit 77 to skip.
# BYRO_CLOUD_SEED selects the same cloud field for every capture (default 0). Other engine variables pass through, so
# under one such as BYRO_HAZE_FLOOR=off the verdict describes that configuration, not the shipped default.
# BYRO_CONTENT_CAMERA_POS and BYRO_CONTENT_CAMERA_FORWARD together replace one fixture's view, for scouting: no
# sample region applies, so that run captures 12:00, 07:00 and 17:30, prints radiance statistics only and judges
# nothing, and BYRO_FROXEL_SUN_OCCLUSION passes through from the caller.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
BIN="${BYRO_CONTENT_BIN:-$ROOT/target/release/byroredux}"
OUT="${BYRO_CONTENT_OUT:-$(mktemp -d /tmp/byro-shaft-content.XXXXXX)}"
GAMES="${BYROREDUX_GAMES_DIR:-/mnt/data/SteamLibrary/steamapps/common}"
WHICH="${1:-all}"
MODE=views
CAMERA=()
if [[ -n "${BYRO_CONTENT_CAMERA_POS:-}" || -n "${BYRO_CONTENT_CAMERA_FORWARD:-}" ]]; then
    if [[ "$WHICH" == all || -z "${BYRO_CONTENT_CAMERA_POS:-}" || -z "${BYRO_CONTENT_CAMERA_FORWARD:-}" ]]; then
        echo "FAIL: supply both camera vectors for one fixture" >&2
        exit 2
    fi
    MODE=scout
    CAMERA=(--camera-pos "$BYRO_CONTENT_CAMERA_POS" --camera-forward "$BYRO_CONTENT_CAMERA_FORWARD")
fi

[[ -x "$BIN" ]] || { echo "SKIP: engine binary not found at $BIN" >&2; exit 77; }
mkdir -p "$OUT"
printf '%s\n' "${BYRO_CLOUD_SEED:-0}" > "$OUT/cloud_seed.txt"
if [[ -z "${DISPLAY:-}" ]]; then
    command -v xvfb-run >/dev/null || { echo "SKIP: no X display or xvfb-run" >&2; exit 77; }
    RUNNER=(xvfb-run -a)
else
    RUNNER=()
fi

# fixture <game> -> sets DATA_DIR and ARGS for the fixture cell, and its VLM-SH-012 view (named by the owner
# 2026-11-27): VIEW_POS, VIEW_FORWARD, LOW_HOUR and REGION (x0,y0,x1,y1 in the 1280x720 capture, ends excluded).
# The comment above each view names the occluder.
fixture() {
    case "$1" in
    fnv) DATA_DIR="$GAMES/Fallout New Vegas/Data"
         ARGS=(--esm "$DATA_DIR/FalloutNV.esm" --grid 0,0 --radius 1 --wrld WastelandNV
               --bsa "$DATA_DIR/Fallout - Meshes.bsa" --bsa "$DATA_DIR/Update.bsa"
               --textures-bsa "$DATA_DIR/Fallout - Textures.bsa" --textures-bsa "$DATA_DIR/Update.bsa")
         # DLC03SatelliteDish01 (REFR 000E7125)
         VIEW_POS=-891,8327,-1988 VIEW_FORWARD=0.97269,0.17365,0.15406 LOW_HOUR=7 REGION=600,204,700,236 ;;
    fo3) DATA_DIR="$GAMES/Fallout 3 goty/Data"
         ARGS=(--esm "$DATA_DIR/Fallout3.esm" --grid -1,-7 --radius 1 --wrld MegatonWorld
               --bsa "$DATA_DIR/Fallout - Meshes.bsa" --textures-bsa "$DATA_DIR/Fallout - Textures.bsa")
         # MegatonChurchofAtom (REFR 0001CBF2)
         VIEW_POS=-4022,12068,26501 VIEW_FORWARD=0.88773,0.43837,0.14060 LOW_HOUR=7 REGION=571,261,671,293 ;;
    obl) DATA_DIR="$GAMES/Oblivion/Data"
         ARGS=(--esm "$DATA_DIR/Oblivion.esm" --grid 0,0 --radius 1 --wrld Tamriel
               --bsa "$DATA_DIR/Oblivion - Meshes.bsa" --textures-bsa "$DATA_DIR/Oblivion - Textures - Compressed.bsa")
         # TreeSugarMapleForestSU (REFR 0006276B) and neighbouring trees. TEMPORARY: the engine draws them as
         # SpeedTree placeholder billboards, so measure this view again when real tree geometry replaces them.
         VIEW_POS=2048,3380,-1848 VIEW_FORWARD=-0.80671,0.57358,0.14224 LOW_HOUR=17.5 REGION=432,149,532,181 ;;
    sky) DATA_DIR="$GAMES/Skyrim Special Edition/Data"
         ARGS=(--esm "$DATA_DIR/Skyrim.esm" --grid 2,-4 --radius 1 --wrld Tamriel
               --bsa "$DATA_DIR/Skyrim - Meshes0.bsa" --bsa "$DATA_DIR/Skyrim - Meshes1.bsa")
         for i in 0 1 2 3 4 5 6 7 8; do ARGS+=(--textures-bsa "$DATA_DIR/Skyrim - Textures$i.bsa"); done
         # SFarmhouseWindMillDirt02Whiterun (REFR 0005CA6C), in cell (4,-5): outside the first loaded grid,
         # streamed in as the engine follows the camera.
         VIEW_POS=16747,-4350,18321 VIEW_FORWARD=0.93935,0.30902,0.14878 LOW_HOUR=7 REGION=616,226,716,258 ;;
    fo4) DATA_DIR="$GAMES/Fallout 4/Data"
         ARGS=(--esm "$DATA_DIR/Fallout4.esm" --grid 0,0 --radius 1 --wrld Commonwealth
               --bsa "$DATA_DIR/Fallout4 - Meshes.ba2" --bsa "$DATA_DIR/Fallout4 - MeshesExtra.ba2")
         for i in 1 2 3 4 5 6 7 8 9; do ARGS+=(--textures-bsa "$DATA_DIR/Fallout4 - Textures$i.ba2"); done
         ARGS+=(--textures-bsa "$DATA_DIR/Fallout4 - TexturesPatch.ba2" --materials-ba2 "$DATA_DIR/Fallout4 - Materials.ba2")
         # Brick building in precombined geometry (no per-object REFR)
         VIEW_POS=2048,964,-1848 VIEW_FORWARD=0.80907,0.57358,0.12814 LOW_HOUR=7 REGION=724,396,824,428 ;;
    esac
    [[ -d "$DATA_DIR" ]]
}

capture() { # game hour kind(final|radiance) [shadow(on|off)] [tag]
    local game="$1" hour="$2" kind="$3" shadow="${4:-}" debug=() occlusion=()
    local name="${game}_${hour}_${kind}${4:+_$4}${5:+_$5}"
    [[ "$kind" == radiance ]] && debug=(--render-debug-mode volumetric_radiance)
    case "$shadow" in
    on) occlusion=(BYRO_FROXEL_SUN_OCCLUSION=1) ;;
    off) occlusion=(BYRO_FROXEL_SUN_OCCLUSION=0) ;;
    esac
    ( cd "$DATA_DIR" && env -u WAYLAND_DISPLAY XDG_SESSION_TYPE=x11 BYRO_DEBUG_SERVER=0 BYRO_HOUR="$hour" BYRO_CLOUD_SEED="${BYRO_CLOUD_SEED:-0}" \
        "${occlusion[@]}" BYROREDUX_SETTINGS_PATH="$OUT/settings_${name}.toml" \
        "${RUNNER[@]}" timeout 300s "$BIN" "${ARGS[@]}" --bench-mode renderer-stepped --bench-camera static \
        --bench-frames 40 --upscaler taa --window-size 1280x720 "${CAMERA[@]}" "${debug[@]}" \
        --screenshot "$OUT/${name}.png" ) >"$OUT/${name}.log" 2>&1 \
        || { echo "FAIL: $name; see $OUT/${name}.log" >&2; exit 1; }
}

games=(fnv fo3 obl sky fo4)
[[ "$WHICH" != all ]] && games=("$WHICH")
ran=()
for game in "${games[@]}"; do
    if ! fixture "$game"; then echo "smoke[exterior-shafts-content]: SKIP -- missing $DATA_DIR" >&2; continue; fi
    if [[ "$MODE" == scout ]]; then
        for hour in 12 7 17.5; do
            capture "$game" "$hour" radiance
            capture "$game" "$hour" final
        done
        ran+=("$game")
    else
        CAMERA=(--camera-pos "$VIEW_POS" --camera-forward "$VIEW_FORWARD")
        for hour in 12 "$LOW_HOUR"; do
            for shadow in on off; do
                capture "$game" "$hour" radiance "$shadow"
                capture "$game" "$hour" final "$shadow"
            done
        done
        for shadow in on off; do capture "$game" "$LOW_HOUR" radiance "$shadow" repeat; done
        ran+=("$game:$LOW_HOUR:$REGION")
    fi
done
# A full A8 measurement needs all five fixtures.
(( ${#ran[@]} > 0 )) || exit 77
if [[ "$WHICH" == all && ${#ran[@]} -ne ${#games[@]} ]]; then
    echo "SKIP: A8 requires all five fixtures; only ${#ran[@]} ran" >&2
    exit 77
fi

python3 -I - "$OUT" "$MODE" "${ran[@]}" <<'PY'
import sys
from pathlib import Path

import numpy as np
from PIL import Image

out, mode, ran = Path(sys.argv[1]), sys.argv[2], sys.argv[3:]


def frame(name):
    pixels = np.asarray(Image.open(out / f"{name}.png").convert("RGB")).astype(float)
    if pixels.shape != (720, 1280, 3):
        sys.exit(f"FAIL: {name}: capture resolution is not 1280x720")
    return pixels


def radiance(name):  # the mapped view inverted to linear
    return -np.log(1 - np.clip(frame(name).mean(axis=2) / 255, 0, 0.999))


def statistics(game, hours, suffix):
    cells = []
    for hour in hours:
        x = radiance(f"{game}_{hour}_radiance{suffix}")
        final = frame(f"{game}_{hour}_final{suffix}").mean()
        cells.append(f"{hour:>4}h {x.mean():.4f} / {np.percentile(x, 95):.4f} / {np.percentile(x, 5):.4f}; {final:6.1f}")
    print(f"{game:4s} " + " | ".join(cells))


views = [spec.split(":") for spec in ran] if mode == "views" else []
print("in-scatter radiance, linear (mapped view inverted): frame mean / p95 / p5; finished-frame mean")
if mode == "scout":
    for game in ran:
        statistics(game, ("12", "7", "17.5"), "")
for game, low, _ in views:
    statistics(game, ("12", low), "_on")

failed = []
if views:
    print("shaft contrast over each view's sample region, 1 - mean(shadow on) / mean(shadow off);")
    print("content rule at low sun: at least 0.05 and more than twice |value - repeat|")
for game, low, region in views:
    x0, y0, x1, y1 = map(int, region.split(","))

    def contrast(hour, tag=""):
        on, off = (radiance(f"{game}_{hour}_radiance_{s}{tag}")[y0:y1, x0:x1].mean() for s in ("on", "off"))
        return 1 - on / off if off > 0 else float("nan")  # no in-scatter in the region fails the rule

    noon, value, repeat = contrast("12"), contrast(low), contrast(low, "_repeat")
    noise = abs(value - repeat)
    passed = value >= 0.05 and value > 2 * noise
    on, off = (frame(f"{game}_{low}_final_{s}").mean(axis=2) for s in ("on", "off"))
    changed = 100 * (np.abs(on - off) > 4).mean()
    print(f"{game:4s} noon {noon:+.4f} | {low:>4}h {value:+.4f}, repeat {repeat:+.4f}, noise {noise:.4f} | "
          f"finished-frame pixels the shadow changes by > 4 of 255: {changed:4.1f} % | {'pass' if passed else 'FAIL'}")
    if not passed:
        failed.append(game)

print(f"Captures and logs: {out}")
if failed:
    sys.exit(f"FAIL: content contrast rule not met: {', '.join(failed)}")
if views:
    print(f"PASS: content contrast rule met on {', '.join(game for game, _, _ in views)}")
PY
