#!/usr/bin/env bash
# M43 gate — crafting at a workbench raises CRFT (Craft Item), live.
#
#   1. Boot the Skyrim SE profile onto the Whiterun exterior grid with a player
#      (`Installed N crafting recipes`: Skyrim's COBJ recipes and the FURN bench keywords).
#   2. `craft list` names the recipes the placed bench nearest the player offers;
#      `inv.add` gives the player leather and `craft make` crafts leather strips
#      through the crafting page's own action: the ingredient leaves the inventory,
#      the item joins it.
#   3. The Story Manager's CRFT carries the bench in R1, so a radiant quest keyed on
#      it can start: `started quest ... on 'CRFT' event` (WICraftItemNode). The nodes
#      under CRFT roll a random percent, so the gate crafts repeatedly until one starts.
#
# Attribution is airtight: nothing but a craft raises CRFT.
set -euo pipefail

PORT="${BYRO_DEBUG_PORT:-19880}"
TIMEOUT="${BYROREDUX_SMOKE_TIMEOUT:-300}"
LEATHER="${BYRO_SMOKE_LEATHER:-0x000DB5D2}"
RECIPE="${BYRO_SMOKE_RECIPE:-0x000F7F43}"
LOG_DIR="$(mktemp -d /tmp/byro-m43-craft.XXXXXX)"
engine_pid=""

stop_engine() {
    if [[ -n "$engine_pid" ]]; then
        kill -TERM "$engine_pid" 2>/dev/null || true
        wait "$engine_pid" 2>/dev/null || true
        engine_pid=""
    fi
}
trap stop_engine EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

fail() {
    echo "smoke[m43-crafting]: FAIL -- $*" >&2
    echo "artifacts: $LOG_DIR" >&2
    exit 1
}

# #5427 — SKIP != PASS: a runner without the game's data measures nothing.
DATA="${BYROREDUX_SKYRIMSE_DATA:-/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data}"
for f in "Skyrim.esm" "Skyrim - Meshes0.bsa" "Skyrim - Textures0.bsa"; do
    if [ ! -f "$DATA/$f" ]; then
        echo "smoke[m43-crafting]: SKIP -- missing $DATA/$f"
        exit 77
    fi
done

if ss -ltn "sport = :$PORT" | grep -q LISTEN; then
    fail "port $PORT already occupied; refusing to attach to another session"
fi

cargo build --release --offline --quiet -p byroredux -p byro-dbg \
    || fail "release build failed"

wait_log() {
    local file="$1" pattern="$2" deadline=$((SECONDS + TIMEOUT))
    until grep -Eq "$pattern" "$file"; do
        kill -0 "$engine_pid" 2>/dev/null || fail "engine exited waiting for $pattern"
        (( SECONDS < deadline )) || fail "timeout waiting for $pattern"
        sleep 0.5
    done
}

debug_command() {
    timeout 60 env BYRO_DEBUG_PORT="$PORT" target/release/byro-dbg >"$2" 2>&1 <<EOF2
$1
.quit
EOF2
}

env BYRO_DEBUG_PORT="$PORT" BYRO_DEBUG_SERVER=1 \
    BYROREDUX_SETTINGS_PATH="$LOG_DIR/settings.toml" \
    RUST_LOG="warn,byroredux_scripting=info,byroredux::asset_provider=info,byroredux::cell_loader::created=info" \
    target/release/byroredux \
    --game skyrim_se --grid 2,-4 --radius 2 \
    --player --bench-frames 30 --bench-hold \
    >"$LOG_DIR/session.stdout" 2>"$LOG_DIR/session.stderr" &
engine_pid=$!

wait_log "$LOG_DIR/session.stderr" 'Installed [0-9]+ crafting recipes'
wait_log "$LOG_DIR/session.stderr" 'Installed Story Manager tree: 571 nodes'
# Let the exterior grid finish loading so a bench is resident.
sleep 15

debug_command "craft list" "$LOG_DIR/list.log" || fail "craft unavailable"
grep -Fq 'recipe(s)' "$LOG_DIR/list.log" || fail "no bench near the player: $(sed -n '1,6p' "$LOG_DIR/list.log")"

CRAFTS="${BYRO_SMOKE_CRAFTS:-25}"
debug_command "inv.add $LEATHER $CRAFTS" "$LOG_DIR/add.log" || fail "inv.add unavailable"
started=0
for attempt in $(seq 1 "$CRAFTS"); do
    debug_command "craft make $RECIPE" "$LOG_DIR/make.log" || fail "craft unavailable"
    grep -Fq 'Crafted' "$LOG_DIR/make.log" || fail "the craft was refused: $(sed -n '1,6p' "$LOG_DIR/make.log")"
    for _ in $(seq 1 6); do
        sleep 0.5
        if grep -Eq "started quest 0x[0-9A-F]+ \('[^']+'\) via node '[^']+' on 'CRFT' event" "$LOG_DIR/session.stderr"; then
            started=1
            break 2
        fi
    done
done
[[ $started -eq 1 ]] || fail "$CRAFTS crafts started no quest on CRFT"

if grep -Fq 'panicked' "$LOG_DIR/session.stderr"; then
    fail "the engine panicked: $(grep -F 'panicked' "$LOG_DIR/session.stderr" | head -1)"
fi

stop_engine
echo "smoke[m43-crafting]: PASS -- a craft at the nearest bench consumed its ingredient,"
echo "yielded the item, and CRFT started a quest on it."
echo "artifacts: $LOG_DIR"
