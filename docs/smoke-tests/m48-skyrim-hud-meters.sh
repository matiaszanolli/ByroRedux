#!/usr/bin/env bash
# M48 Phase 2 A4 (#6661, GUI-HUD-001/003) — the vanilla Skyrim HUD meters follow the player's
# vitals through the path service. Same fixture as m48-6 (BleakFallsBarrow01, `--hud`).
#
# `hud.values 1.0 1.0 1.0`, capture, `hud.values 0.35 1.0 1.0`, capture: the health meter band
# (bottom centre) must change and the stamina band (bottom right, pinned 1.0 both times) must not.
# `hud.debug` must show the health feed landed. Bands are the meters' bounds at the 1280x720
# capture, measured from the movie (health 518..763 x 623..635 moves 1610 px for 100 -> 35).
#
# Pre-conditions: Skyrim SE installed (BYROREDUX_SKYRIMSE_DATA or the default path), a Vulkan
# device, Xvfb, a debug build (`cargo build`, with byro-dbg).
# Exit: 0 PASS, 77 SKIP (game data absent), non-zero FAIL.

set -euo pipefail
GATE=m48-skyrim-hud-meters
DATA="${BYROREDUX_SKYRIMSE_DATA:-/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data}"
PORT="${BYRO_DEBUG_PORT:-9916}"
for f in "Skyrim.esm" "Skyrim - Meshes0.bsa" "Skyrim - Textures0.bsa" "Skyrim - Interface.bsa"; do
    [ -f "$DATA/$f" ] || { echo "smoke[$GATE]: SKIP -- missing $DATA/$f"; exit 77; }
done
ENGINE_ARGS=(--esm "Skyrim.esm" --cell BleakFallsBarrow01 --bsa "Skyrim - Meshes0.bsa" --textures-bsa "Skyrim - Textures0.bsa")
FULL="hud.values 1.0 1.0 1.0"
LOW="hud.values 0.35 1.0 1.0"
FEED_TOKEN="_root.HUDMovieBaseInstance.SetHealthMeterPercent"
BANDS='{"health": (518, 764, 621, 636, "moves"), "stamina": (893, 1141, 621, 633, "still")}'
source "$(dirname "$0")/lib/hud_meters.sh"
