#!/usr/bin/env bash
# M48 Phase 2 A5 (#6661, GUI-HUD-002/003) — the vanilla Fallout 4 HUD health and action-point
# meters follow the player's vitals through the path service. Same fixture as m48-7
# (MedTekResearch01, `--hud`).
#
# `hud.values 1.0 1.0`, capture, `hud.values 0.35 0.35`, capture: the HP band (bottom left) and
# the AP band (bottom right) must change, the compass band (top centre; the widget has no
# ActionScript, so nothing drives it — #7036) must not. `hud.debug` must show the HP feed landed.
# Bands are the bars' bounds at the 1280x720 capture, measured from the movie (HP fill
# 96..295 x 652..657, AP its mirror).
#
# Pre-conditions: Fallout 4 installed (BYROREDUX_FO4_DATA or the default path), a Vulkan device,
# Xvfb, a debug build (`cargo build`, with byro-dbg).
# Exit: 0 PASS, 77 SKIP (game data absent), non-zero FAIL.

set -euo pipefail
GATE=m48-fo4-hud-meters
DATA="${BYROREDUX_FO4_DATA:-/mnt/data/SteamLibrary/steamapps/common/Fallout 4/Data}"
PORT="${BYRO_DEBUG_PORT:-9917}"
for f in "Fallout4.esm" "Fallout4 - Meshes.ba2" "Fallout4 - Textures1.ba2" "Fallout4 - Interface.ba2"; do
    [ -f "$DATA/$f" ] || { echo "smoke[$GATE]: SKIP -- missing $DATA/$f"; exit 77; }
done
ENGINE_ARGS=(--esm "Fallout4.esm" --cell MedTekResearch01 --bsa "Fallout4 - Meshes.ba2" --bsa "Fallout4 - MeshesExtra.ba2"
    --textures-bsa "Fallout4 - Textures1.ba2" --materials-ba2 "Fallout4 - Materials.ba2")
FULL="hud.values 1.0 1.0"
LOW="hud.values 0.35 0.35"
FEED_TOKEN="LeftMeters_mc.HPMeter_mc.MeterBar_mc.Percent"
BANDS='{"health": (90, 300, 650, 660, "moves"), "ap": (980, 1190, 650, 660, "moves"), "compass": (500, 780, 28, 75, "still")}'
source "$(dirname "$0")/lib/hud_meters.sh"
