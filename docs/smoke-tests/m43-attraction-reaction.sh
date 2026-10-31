#!/usr/bin/env bash
# M43 gate — an actor reacts to an attraction object it shares a keyword with, live (Fallout 4).
#
#   Fallout4.esm places no matching NPC and object together in a plain cell (settlers and companions meet their
#   objects at runtime), so the gate brings a real pair together: `AO_DeaconGiveInfo_Object` (0x1761C3, an
#   ACTI carrying keyword AO_Type_DeaconGiveInfo) and `DeaconAtGoodneighbor` (0x1A0422, an NPC carrying the same
#   keyword), both created beside the player with `place.at`. `attraction.status` counts them, and after the
#   rule's delay the engine logs `attraction: actor ... reacts to object ...; raising AOBJ`.
set -euo pipefail

PORT="${BYRO_DEBUG_PORT:-19889}"
TIMEOUT="${BYROREDUX_SMOKE_TIMEOUT:-300}"
CELL="${BYRO_SMOKE_ATTRACTION_CELL:-AndrewStation01}"
OBJECT="${BYRO_SMOKE_AO_OBJECT:-0x001761C3}"
NPC="${BYRO_SMOKE_AO_NPC:-0x001A0422}"
LOG_DIR="$(mktemp -d /tmp/byro-m43-attraction-reaction.XXXXXX)"
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
    echo "smoke[m43-attraction-reaction]: FAIL -- $*" >&2
    echo "artifacts: $LOG_DIR" >&2
    exit 1
}

# #5427 — SKIP != PASS: a runner without the game's data measures nothing.
DATA="${BYROREDUX_FO4_DATA:-/mnt/data/SteamLibrary/steamapps/common/Fallout 4/Data}"
for f in "Fallout4.esm" "Fallout4 - Meshes.ba2" "Fallout4 - Textures1.ba2"; do
    if [ ! -f "$DATA/$f" ]; then
        echo "smoke[m43-attraction-reaction]: SKIP -- missing $DATA/$f"
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
    sed -i 's/\\n/\n/g' "$2"
}

env BYRO_DEBUG_PORT="$PORT" BYRO_DEBUG_SERVER=1 \
    BYROREDUX_SETTINGS_PATH="$LOG_DIR/settings.toml" \
    RUST_LOG="warn,byroredux::systems::attraction=info" \
    target/release/byroredux \
    --esm "$DATA/Fallout4.esm" \
    --bsa "$DATA/Fallout4 - Meshes.ba2" --bsa "$DATA/Fallout4 - MeshesExtra.ba2" \
    --textures-bsa "$DATA/Fallout4 - Textures1.ba2" \
    --materials-ba2 "$DATA/Fallout4 - Materials.ba2" \
    --cell "$CELL" --player --bench-frames 30 --bench-hold \
    >"$LOG_DIR/session.stdout" 2>"$LOG_DIR/session.stderr" &
engine_pid=$!

wait_log "$LOG_DIR/session.stderr" 'bench-hold:'
sleep 4

debug_command "place.at $OBJECT 60 0 0" "$LOG_DIR/place-object.log" || fail "place.at unavailable"
grep -Fq 'placed base' "$LOG_DIR/place-object.log" || fail "the object was not placed: $(sed -n '1,4p' "$LOG_DIR/place-object.log")"
debug_command "place.at $NPC 0 60 0" "$LOG_DIR/place-npc.log" || fail "place.at unavailable"
grep -Fq 'placed base' "$LOG_DIR/place-npc.log" || fail "the NPC was not placed: $(sed -n '1,4p' "$LOG_DIR/place-npc.log")"

# Both must appear in the attraction scan before a reaction is possible.
deadline=$((SECONDS + 60))
until debug_command "attraction.status" "$LOG_DIR/status.log" && grep -Eq 'placed objects: [1-9][0-9]*; live actors: [1-9]' "$LOG_DIR/status.log"; do
    (( SECONDS < deadline )) || fail "the placed pair never showed in the attraction scan: $(grep -E 'attraction rules' "$LOG_DIR/status.log" | head -1)"
    sleep 2
done

wait_log "$LOG_DIR/session.stderr" 'attraction: actor .* reacts to object .*; raising AOBJ'

if grep -Fq 'panicked' "$LOG_DIR/session.stderr"; then
    fail "the engine panicked: $(grep -F 'panicked' "$LOG_DIR/session.stderr" | head -1)"
fi

stop_engine
echo "smoke[m43-attraction-reaction]: PASS -- $(grep -E 'attraction rules' "$LOG_DIR/status.log" | head -1 | sed 's/^byro> "//; s/"$//'); an actor reacted to its object and raised AOBJ."
echo "artifacts: $LOG_DIR"
