#!/usr/bin/env bash
# M43 gate — dropping an item raises REMP (Player Remove Item), live.
#
#   1. Boot the Skyrim SE profile into WhiterunDragonsReach with a player.
#   2. `inv.add` gives the player two of a pickup item and `inv.drop` queues the
#      inventory menu's own Drop action: one item leaves the inventory and a
#      created reference of the same base is placed at the player's feet.
#   3. The placement spawns (`Spawned created reference ...`) and only then does
#      the Story Manager's REMP carry that reference in R2, so a radiant quest
#      keyed on it can start: `started quest ... on 'REMP' event`
#      (Skyrim.esm authors 5 quests under REMP; `WIPlayerRemoveItemAnyNode` takes
#      the first).
#
# Attribution is airtight: nothing but the drop raises REMP.
set -euo pipefail

PORT="${BYRO_DEBUG_PORT:-19879}"
TIMEOUT="${BYROREDUX_SMOKE_TIMEOUT:-300}"
ITEM="${BYRO_SMOKE_ITEM:-0x00012EB7}"
LOG_DIR="$(mktemp -d /tmp/byro-m43-drop.XXXXXX)"
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
    echo "smoke[m43-item-drop]: FAIL -- $*" >&2
    echo "artifacts: $LOG_DIR" >&2
    exit 1
}

# #5427 — SKIP != PASS: a runner without the game's data measures nothing.
DATA="${BYROREDUX_SKYRIMSE_DATA:-/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data}"
for f in "Skyrim.esm" "Skyrim - Meshes0.bsa" "Skyrim - Textures0.bsa"; do
    if [ ! -f "$DATA/$f" ]; then
        echo "smoke[m43-item-drop]: SKIP -- missing $DATA/$f"
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
    --game skyrim_se --cell WhiterunDragonsReach \
    --player --bench-frames 30 --bench-hold \
    >"$LOG_DIR/session.stdout" 2>"$LOG_DIR/session.stderr" &
engine_pid=$!

wait_log "$LOG_DIR/session.stderr" 'Installed Story Manager tree: 571 nodes'

# The radiant nodes under REMP gate on a random-percent roll, so one drop may start nothing: drop
# again (up to $DROPS times, a second or two apart) until a quest starts. Every drop still has to
# spawn its reference.
DROPS="${BYRO_SMOKE_DROPS:-25}"
debug_command "inv.add $ITEM $DROPS" "$LOG_DIR/add.log" || fail "inv.add unavailable"
grep -Fq 'inv.add:' "$LOG_DIR/add.log" || fail "inv.add did not answer: $(sed -n '1,4p' "$LOG_DIR/add.log")"
started=0
for attempt in $(seq 1 "$DROPS"); do
    debug_command "inv.drop $ITEM" "$LOG_DIR/drop.log" || fail "inv.drop unavailable"
    grep -Fq 'queued drop' "$LOG_DIR/drop.log" || fail "inv.drop did not queue: $(sed -n '1,4p' "$LOG_DIR/drop.log")"
    for _ in $(seq 1 8); do
        sleep 0.5
        if grep -Eq "started quest 0x[0-9A-F]+ \('[^']+'\) via node '[^']+' on 'REMP' event" "$LOG_DIR/session.stderr"; then
            started=1
            break 2
        fi
    done
done
grep -Eq 'Spawned created reference [0-9A-F]+' "$LOG_DIR/session.stderr" || fail "no dropped reference ever spawned"
[[ $started -eq 1 ]] || fail "$DROPS drops started no quest on REMP"

if grep -Fq 'panicked' "$LOG_DIR/session.stderr"; then
    fail "the engine panicked: $(grep -F 'panicked' "$LOG_DIR/session.stderr" | head -1)"
fi

stop_engine
echo "smoke[m43-item-drop]: PASS -- a dropped item spawned as a reference and REMP"
echo "started a quest on it."
echo "artifacts: $LOG_DIR"
