#!/usr/bin/env bash
# Owner-run live gate for the player's bow on Oblivion, player-archery PRD §7.2 scenario 4 (#7055): WeapIronBow
# (00025231) and Arrow1Iron (00017829) in Anvil's Count's Arms; `bow.shoot 2` (a draw of two seconds, a full draw) at the
# first patron lowers their Health, spends one arrow and feeds Marksman (`levelup`); a ceiling shot feeds nothing.
# The shared body is lib/archery.sh. Requires a Vulkan device, display, and BYROREDUX_OBLIVION_DATA.
set -euo pipefail
GATE=oblivion-archery
DATA="${BYROREDUX_OBLIVION_DATA:-}"
if [[ -z "$DATA" || ! -f "$DATA/Oblivion.esm" || ! -f "$DATA/Oblivion - Meshes.bsa" || ! -f "$DATA/Oblivion - Textures - Compressed.bsa" ]]; then
    echo "smoke[$GATE]: SKIP -- missing BYROREDUX_OBLIVION_DATA (set it to the Oblivion Data directory)"
    exit 77
fi
DATA="$(cd "$DATA" && pwd)"
PORT_DEFAULT=19883
ENGINE_ARGS=(--esm "$DATA/Oblivion.esm" --cell AnvilTheCountsArms
    --bsa "$DATA/Oblivion - Meshes.bsa" --textures-bsa "$DATA/Oblivion - Textures - Compressed.bsa")
BOW=0x00025231
ARROW=0x00017829
SHOOT_ARGS=2
FEED='feed=Marksman fed'
LEVEL_SKILL=Marksman
source "$(dirname "$0")/lib/archery.sh"
