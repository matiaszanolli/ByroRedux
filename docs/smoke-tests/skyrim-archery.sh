#!/usr/bin/env bash
# Owner-run live gate for the player's bow on Skyrim, player-archery PRD §7.2 scenario 4 (#7055): HuntingBow (00013985)
# and IronArrow (0001397D) in the Bannered Mare; `bow.shoot` (a full draw) at the first patron lowers their Health and
# spends one arrow, a ceiling shot hurts nobody, and `bow.status` reports `Archery unfed` (Q3: Skyrim has no weapon-skill
# emitter). The shared body is lib/archery.sh. Requires a Vulkan device, display, and Skyrim SE's data
# (BYROREDUX_SKYRIMSE_DATA, defaulting to the Steam install).
set -euo pipefail
GATE=skyrim-archery
DATA="${BYROREDUX_SKYRIMSE_DATA:-/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data}"
for f in "Skyrim.esm" "Skyrim - Meshes0.bsa" "Skyrim - Textures0.bsa"; do
    if [[ ! -f "$DATA/$f" ]]; then
        echo "smoke[$GATE]: SKIP -- missing $DATA/$f"
        exit 77
    fi
done
PORT_DEFAULT=19884
ENGINE_ARGS=(--game skyrim_se --cell WhiterunBanneredMare)
BOW=0x00013985
ARROW=0x0001397D
SHOOT_ARGS=
FEED='feed=Archery unfed'
LEVEL_SKILL=
HEALTH_MODE=cond
source "$(dirname "$0")/lib/archery.sh"
