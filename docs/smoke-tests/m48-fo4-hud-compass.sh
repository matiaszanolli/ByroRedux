#!/usr/bin/env bash
# M48 GUI-HUD-005 for Fallout 4 (#7036) — the vanilla compass follows the heading. Same fixture as
# m48-fo4-hud-meters.sh (MedTekResearch01, `--hud`).
#
# The widget has no ActionScript; the engine attaches the movie's four DirectionMarkerWidget clips
# and places them by the native HUDCompass rule (x = -halfW * delta / 50 deg, hidden from |delta| >=
# 50 deg; byroredux/src/scaleform_hud.rs, Fallout4Compass). `hud.heading 90`, capture,
# `hud.heading 110`, capture: the East marker's tick (a 2x7 px white mark under the bar, holder
# origin at (640, 659) of the 1280x720 capture) sits at the centre, then 60 px left of it
# (-151 * 20 / 50). No marker lies right of the centre in either capture (North and South are 90
# and 70 deg away). The bands count white HUD ink, not differences: the tick is too small to tell
# from the cell's RT shimmer by differencing.
#
# `hud.heading 90` vs `270` (the issue's wording) puts E, then W, at the same centre: the two
# captures differ only in the letter, which this gate leaves alone (the letters draw in the Noto
# Sans fallback since #6665, until Fallout 4's font classes are mapped, M48 Phase 6).
#
# Pre-conditions: Fallout 4 installed (BYROREDUX_FO4_DATA or the default path), a Vulkan device,
# Xvfb, a debug build (`cargo build`, with byro-dbg).
# Exit: 0 PASS, 77 SKIP (game data absent), non-zero FAIL.

set -euo pipefail
GATE=m48-fo4-hud-compass
DATA="${BYROREDUX_FO4_DATA:-/mnt/data/SteamLibrary/steamapps/common/Fallout 4/Data}"
PORT="${BYRO_DEBUG_PORT:-9918}"
for f in "Fallout4.esm" "Fallout4 - Meshes.ba2" "Fallout4 - Textures1.ba2" "Fallout4 - Interface.ba2"; do
    [ -f "$DATA/$f" ] || { echo "smoke[$GATE]: SKIP -- missing $DATA/$f"; exit 77; }
done
ENGINE_ARGS=(--esm "Fallout4.esm" --cell MedTekResearch01 --bsa "Fallout4 - Meshes.ba2" --bsa "Fallout4 - MeshesExtra.ba2"
    --textures-bsa "Fallout4 - Textures1.ba2" --materials-ba2 "Fallout4 - Materials.ba2")
FULL="hud.heading 90"
LOW="hud.heading 110"
FEED_TOKEN="BottomCenterGroup_mc.CompassWidget_mc.OtherMarkerHolder_mc.ByroCompassEast.x"
BANDS='{"centre": (636, 645, 660, 667, "ink-full"), "east-at-20": (574, 587, 660, 667, "ink-low"), "right-of-centre": (646, 792, 660, 667, "no-ink"), "far-left": (488, 573, 660, 667, "no-ink")}'
source "$(dirname "$0")/lib/hud_meters.sh"
