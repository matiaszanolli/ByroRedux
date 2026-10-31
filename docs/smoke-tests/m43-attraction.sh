#!/usr/bin/env bash
# M43 gate — the attraction system reads Fallout4.esm's AORU rules, keywords and carriers, live.
#
#   Boot the FO4 interior `AndrewStation01` with a player. `attraction.status` reports the attraction
#   keywords the engine models (15 in the master minus the 2 on a Combat Target rule), the object and NPC
#   bases that carry an attraction keyword (KYWD DATA -> AORU; KWDA on ACTI / FLOR / FURN / CONT / NPC_),
#   and how many are placed in the loaded cell. It proves the parse plumbing against the real master (the
#   reaction is `m43-attraction-reaction.sh`'s). `sm.nodes` then lists the 17 vanilla Story Manager nodes that
#   carry a node-level reset window (HNAM) with their authored hours.
set -euo pipefail

PORT="${BYRO_DEBUG_PORT:-19882}"
TIMEOUT="${BYROREDUX_SMOKE_TIMEOUT:-300}"
CELL="${BYRO_SMOKE_ATTRACTION_CELL:-AndrewStation01}"
LOG_DIR="$(mktemp -d /tmp/byro-m43-attraction.XXXXXX)"
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
    echo "smoke[m43-attraction]: FAIL -- $*" >&2
    echo "artifacts: $LOG_DIR" >&2
    exit 1
}

# #5427 — SKIP != PASS: a runner without the game's data measures nothing.
DATA="${BYROREDUX_FO4_DATA:-/mnt/data/SteamLibrary/steamapps/common/Fallout 4/Data}"
for f in "Fallout4.esm" "Fallout4 - Meshes.ba2" "Fallout4 - Textures1.ba2"; do
    if [ ! -f "$DATA/$f" ]; then
        echo "smoke[m43-attraction]: SKIP -- missing $DATA/$f"
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
    RUST_LOG="warn" \
    target/release/byroredux \
    --esm "$DATA/Fallout4.esm" \
    --bsa "$DATA/Fallout4 - Meshes.ba2" --bsa "$DATA/Fallout4 - MeshesExtra.ba2" \
    --textures-bsa "$DATA/Fallout4 - Textures1.ba2" \
    --materials-ba2 "$DATA/Fallout4 - Materials.ba2" \
    --cell "$CELL" --player --bench-frames 30 --bench-hold \
    >"$LOG_DIR/session.stdout" 2>"$LOG_DIR/session.stderr" &
engine_pid=$!

wait_log "$LOG_DIR/session.stderr" 'bench-hold:'
sleep 3

debug_command "attraction.status" "$LOG_DIR/status.log" || fail "attraction.status unavailable"
line="$(grep -E 'attraction rules:' "$LOG_DIR/status.log" | head -1 || true)"
[[ -n "$line" ]] || fail "no status line: $(sed -n '1,6p' "$LOG_DIR/status.log")"
rules="$(sed -E 's/.*attraction rules: ([0-9]+);.*/\1/' <<<"$line")"
objects="$(sed -E 's/.*object bases: ([0-9]+);.*/\1/' <<<"$line")"
npcs="$(sed -E 's/.*npc bases: ([0-9]+);.*/\1/' <<<"$line")"
# 15 attraction keywords carry the 8 AORU rules; the two on the Combat Target rule (WorkshopResourceObjectRule)
# are left out, so 13 keywords are modelled.
[[ "$rules" -ge 12 ]] || fail "expected the vanilla attraction rules to load, got $rules: $line"
[[ "$objects" -ge 10 ]] || fail "too few attraction object bases ($objects): $line"
[[ "$npcs" -ge 50 ]] || fail "too few attraction NPC bases ($npcs): $line"

# Node-level HNAM reset windows (Fallout 4's "Hours until reset"): the 17 vanilla SMQN nodes that carry one load with
# their authored hours (census of Fallout4.esm: 0.5-140 h, e.g. REChokepointQuests 24, MinutemenRandomEvents 72).
debug_command "sm.nodes" "$LOG_DIR/nodes.log" || fail "sm.nodes unavailable"
grep -Eq '17 of [0-9]+ node\(s\) carry a reset window' "$LOG_DIR/nodes.log" \
    || fail "expected 17 nodes with a reset window: $(sed -n '1,4p' "$LOG_DIR/nodes.log")"
grep -Eq '^node REChokepointQuests reset 24 h ' "$LOG_DIR/nodes.log" || fail "REChokepointQuests should reset after 24 h: $(grep -E 'REChokepoint' "$LOG_DIR/nodes.log")"
grep -Eq '^node MinutemenRandomEvents reset 72 h ' "$LOG_DIR/nodes.log" || fail "MinutemenRandomEvents should reset after 72 h"
nodes="$(grep -E 'node\(s\) carry a reset window' "$LOG_DIR/nodes.log" | head -1 | sed 's/^byro> "//')"

if grep -Fq 'panicked' "$LOG_DIR/session.stderr"; then
    fail "the engine panicked: $(grep -F 'panicked' "$LOG_DIR/session.stderr" | head -1)"
fi

stop_engine
echo "smoke[m43-attraction]: PASS -- $line; $nodes"
echo "artifacts: $LOG_DIR"
