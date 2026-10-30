#!/usr/bin/env bash
# M43 gate — NPC-to-NPC random conversations raise ADIA, live.
#
#   1. Boot the Skyrim SE profile onto the Whiterun exterior grid with a
#      player. The Story Manager tree installs an `ADIA` root (Skyrim.esm
#      authors 786 quests under it).
#   2. Whiterun's NPCs run Sandbox / Guard / Patrol packages whose PKDT
#      interrupt flags allow Random conversations. Each arms a timer between
#      fAISocialTimerForConversationsMin and Max (30-60 s) and, at expiry,
#      looks for a partner in fAISocialRadiusToTriggerConversation, rolling
#      fAISocialchanceForConversation (25 %).
#   3. The producer logs `random conversation: <initiator> -> <partner>` and
#      the Story Manager's own attribution line names the quest, node and
#      event: `started quest ... on 'ADIA' event`. Then the conversation
#      quest's scene must play: a `SCEN ... Playing` transition for it.
#
# Attribution is airtight: the quest can start through no other path (an
# `ADIA` node is reached only by this event), and a pass needs the scene to
# begin, so a start that never plays is a failure.
#
# The dice make the wait random: with a 25 % roll and 30-60 s timers the
# first event usually lands inside a minute and a half. The timeout is long
# enough that missing it is a real failure, not bad luck.
set -euo pipefail

PORT="${BYRO_DEBUG_PORT:-19878}"
TIMEOUT="${BYROREDUX_SMOKE_TIMEOUT:-420}"
LOG_DIR="$(mktemp -d /tmp/byro-m43-adia.XXXXXX)"
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
    echo "smoke[m43-adia-conversations]: FAIL -- $*" >&2
    echo "artifacts: $LOG_DIR" >&2
    exit 1
}

# #5427 — the SKIP≠PASS contract: a runner without the game's data measures
# nothing (exit 77, never a pass).
DATA="${BYROREDUX_SKYRIMSE_DATA:-/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data}"
for f in "Skyrim.esm" "Skyrim - Meshes0.bsa" "Skyrim - Textures0.bsa"; do
    if [ ! -f "$DATA/$f" ]; then
        echo "smoke[m43-adia-conversations]: SKIP -- missing $DATA/$f"
        exit 77
    fi
done

if ss -ltn "sport = :$PORT" | grep -q LISTEN; then
    fail "port $PORT already occupied; refusing to attach to another session"
fi

cargo build --release --offline --quiet -p byroredux \
    || fail "release build failed"

wait_log() {
    local file="$1" pattern="$2" deadline=$((SECONDS + TIMEOUT))
    until grep -Eq "$pattern" "$file"; do
        kill -0 "$engine_pid" 2>/dev/null || fail "engine exited waiting for $pattern"
        (( SECONDS < deadline )) || fail "timeout waiting for $pattern"
        sleep 0.5
    done
}

env BYRO_DEBUG_PORT="$PORT" BYRO_DEBUG_SERVER=1 \
    BYROREDUX_SETTINGS_PATH="$LOG_DIR/settings.toml" \
    RUST_LOG="warn,byroredux_scripting=info,byroredux::asset_provider=info,byroredux::systems::random_conversation=debug,scripting::scene=info" \
    target/release/byroredux \
    --game skyrim_se --grid 2,-4 --radius 2 \
    --player --bench-frames 30 --bench-hold \
    >"$LOG_DIR/session.stdout" 2>"$LOG_DIR/session.stderr" &
engine_pid=$!

# The tree made it into the session with its ADIA root.
wait_log "$LOG_DIR/session.stderr" 'Installed Story Manager tree: 571 nodes'

# The producer raised an event, and the Story Manager started a quest on it.
wait_log "$LOG_DIR/session.stderr" 'random conversation: [0-9]+ -> [0-9]+'
wait_log "$LOG_DIR/session.stderr" "started quest 0x[0-9A-F]+ \('[^']+'\) via node '[^']+' on 'ADIA' event"

# The started quest's scene begins: a Dormant -> Playing transition follows the
# start line. A start whose scene never plays is a conversation nobody hears.
started_line=$(grep -En "on 'ADIA' event" "$LOG_DIR/session.stderr" | head -1 | cut -d: -f1)
deadline=$((SECONDS + 30))
until tail -n +"$started_line" "$LOG_DIR/session.stderr" | grep -Eq 'SCEN [0-9A-F]+ .* Dormant/phase 0 -> Playing'; do
    (( SECONDS < deadline )) || fail "the ADIA-started quest's scene never began playing"
    sleep 0.5
done

# No panic on the way.
if grep -Fq 'panicked' "$LOG_DIR/session.stderr"; then
    fail "the engine panicked: $(grep -F 'panicked' "$LOG_DIR/session.stderr" | head -1)"
fi

stop_engine
echo "smoke[m43-adia-conversations]: PASS -- an NPC pair raised ADIA, the Story Manager"
echo "started a conversation quest on it, and its scene played."
echo "artifacts: $LOG_DIR"
