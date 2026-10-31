#!/usr/bin/env bash
# M43 gate — a Story Manager node's reset window (Fallout 4's node-level "Hours until reset", HNAM), live.
#
#   `MinutemenBOSEvents` (Fallout4.esm) carries a 48 h window, one quest (MinRadiantOwned04_BOS) and a single
#   condition: the script event's keyword must be 0xE479A. So a script event with that keyword reaches it:
#     1. `sm.event SCPT kw=0xE479A` -> `started quest 0x000E477F ... via node 'MinutemenBOSEvents'`.
#     2. `quest.stop` that quest, so it is eligible again and only the window can hold the node back; raising the
#        event again starts nothing.
#     3. `time.advance 49` (past the window) and a third raise starts the quest again.
set -euo pipefail

PORT="${BYRO_DEBUG_PORT:-19901}"
TIMEOUT="${BYROREDUX_SMOKE_TIMEOUT:-300}"
CELL="${BYRO_SMOKE_WINDOW_CELL:-AndrewStation01}"
KEYWORD="${BYRO_SMOKE_WINDOW_KEYWORD:-0x000E479A}"
QUEST="${BYRO_SMOKE_WINDOW_QUEST:-0x000E477F}"
LOG_DIR="$(mktemp -d /tmp/byro-m43-node-window.XXXXXX)"
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
    echo "smoke[m43-node-window]: FAIL -- $*" >&2
    echo "artifacts: $LOG_DIR" >&2
    exit 1
}

# #5427 — SKIP != PASS: a runner without the game's data measures nothing.
DATA="${BYROREDUX_FO4_DATA:-/mnt/data/SteamLibrary/steamapps/common/Fallout 4/Data}"
for f in "Fallout4.esm" "Fallout4 - Meshes.ba2" "Fallout4 - Textures1.ba2"; do
    if [ ! -f "$DATA/$f" ]; then
        echo "smoke[m43-node-window]: SKIP -- missing $DATA/$f"
        exit 77
    fi
done

if ss -ltn "sport = :$PORT" | grep -q LISTEN; then
    fail "port $PORT already occupied; refusing to attach to another session"
fi

cargo build --release --offline --quiet -p byroredux -p byro-dbg \
    || fail "release build failed"

debug_command() {
    timeout 60 env BYRO_DEBUG_PORT="$PORT" target/release/byro-dbg >"$2" 2>&1 <<EOF2
$1
.quit
EOF2
    sed -i 's/\\n/\n/g' "$2"
}

env BYRO_DEBUG_PORT="$PORT" BYRO_DEBUG_SERVER=1 \
    BYROREDUX_SETTINGS_PATH="$LOG_DIR/settings.toml" \
    RUST_LOG="warn,byroredux_scripting=info" \
    target/release/byroredux \
    --esm "$DATA/Fallout4.esm" \
    --bsa "$DATA/Fallout4 - Meshes.ba2" --bsa "$DATA/Fallout4 - MeshesExtra.ba2" \
    --textures-bsa "$DATA/Fallout4 - Textures1.ba2" \
    --materials-ba2 "$DATA/Fallout4 - Materials.ba2" \
    --cell "$CELL" --player --bench-frames 30 --bench-hold \
    >"$LOG_DIR/session.stdout" 2>"$LOG_DIR/session.stderr" &
engine_pid=$!

deadline=$((SECONDS + TIMEOUT))
until grep -Fq 'bench-hold:' "$LOG_DIR/session.stderr"; do
    kill -0 "$engine_pid" 2>/dev/null || fail "engine exited before bench-hold"
    (( SECONDS < deadline )) || fail "timeout waiting for bench-hold"
    sleep 0.5
done
sleep 4

starts() { grep -c "via node 'MinutemenBOSEvents'" "$LOG_DIR/session.stderr" || true; }
raise() { debug_command "sm.event SCPT r1=player kw=$KEYWORD" "$LOG_DIR/raise.log" || fail "sm.event unavailable"; sleep 4; }

raise
[[ "$(starts)" -eq 1 ]] || fail "the first raise should start the node's quest once (starts=$(starts))"

debug_command "quest.stop $QUEST" "$LOG_DIR/stop.log" || fail "quest.stop unavailable"
sleep 2
raise
[[ "$(starts)" -eq 1 ]] || fail "a raise inside the 48 h window started the quest again (starts=$(starts))"

debug_command "time.advance 49" "$LOG_DIR/advance.log" || fail "time.advance unavailable"
sleep 2
raise
[[ "$(starts)" -eq 2 ]] || fail "after 49 h the window should have run out and the quest started again (starts=$(starts))"

if grep -Fq 'panicked' "$LOG_DIR/session.stderr"; then
    fail "the engine panicked: $(grep -F 'panicked' "$LOG_DIR/session.stderr" | head -1)"
fi

stop_engine
echo "smoke[m43-node-window]: PASS -- start, blocked inside the 48 h window with the quest eligible, start again after 49 h."
echo "artifacts: $LOG_DIR"
