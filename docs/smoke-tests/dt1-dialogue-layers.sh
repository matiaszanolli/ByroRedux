#!/usr/bin/env bash
# DT1 gate (#5367 Phases G + F) — dialogue greeting + force-greet, live.
#
#   1. Boot the FNV profile into Doc Mitchell's house with a player.
#   2. Phase G: activate Doc Mitchell (bound to no *running* quest at
#      boot) — pre-#5367 this selected nothing; now the master's
#      generic `GREETING` topic must open the conversation.
#   3. Phase F: install the authored Dialogue-procedure PACK
#      0x000613BB (`CitGunnyGreetPlayer`, PKDD topic 0x000000C8) via
#      `dialogue.forcegreet` — the NPC walks to the player and opens
#      the conversation with the package's topic WITHOUT any
#      activation, through the same selection path.
set -euo pipefail

PORT="${BYRO_DEBUG_PORT:-19878}"
TIMEOUT="${BYROREDUX_SMOKE_TIMEOUT:-300}"
LOG_DIR="$(mktemp -d /tmp/byro-dt1.XXXXXX)"
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
    echo "smoke[dt1-dialogue-layers]: FAIL -- $*" >&2
    echo "artifacts: $LOG_DIR" >&2
    exit 1
}

if ss -ltn "sport = :$PORT" | grep -q LISTEN; then
    fail "port $PORT already occupied; refusing to attach to another session"
fi

cargo build --release --offline --quiet -p byroredux -p byro-dbg \
    || fail "release build failed"

debug_command() {
    timeout "$TIMEOUT" env BYRO_DEBUG_PORT="$PORT" target/release/byro-dbg \
        >"$2" 2>&1 <<EOF
$1
.quit
EOF
}

wait_log() {
    local file="$1" pattern="$2" deadline=$((SECONDS + TIMEOUT))
    until grep -Fq "$pattern" "$file"; do
        kill -0 "$engine_pid" 2>/dev/null || fail "engine exited waiting for $pattern"
        (( SECONDS < deadline )) || fail "timeout waiting for $pattern"
        sleep 0.25
    done
}

env BYRO_DEBUG_PORT="$PORT" BYRO_DEBUG_SERVER=1 \
    BYROREDUX_SETTINGS_PATH="$LOG_DIR/settings.toml" \
    RUST_LOG="warn,byroredux::asset_provider=info" \
    target/release/byroredux \
    --game fnv --cell GSDocMitchellHouse \
    --player --bench-frames 30 --bench-hold \
    >"$LOG_DIR/session.stdout" 2>"$LOG_DIR/session.stderr" &
engine_pid=$!
wait_log "$LOG_DIR/session.stderr" 'bench-hold:'

# ── Phase G: the quest-less activation greets ─────────────────────
debug_command 'find("DocMitchell")' "$LOG_DIR/find.log" || fail 'find unavailable'
entity="$(grep -oE 'Entity [0-9]+' "$LOG_DIR/find.log" | head -1 | cut -d' ' -f2 || true)"
[[ -n "$entity" ]] || fail "Doc Mitchell entity not found in the loaded cell"
echo "smoke[dt1-dialogue-layers]: DocMitchell entity=$entity"

debug_command "script.activate $entity
dialogue.status" "$LOG_DIR/greet.log" || fail 'activation unavailable'
grep -Fq "topic 0x0000C8" "$LOG_DIR/greet.log" \
    || fail "Phase G: activation did not open the generic GREETING topic: $(sed -n '1,8p' "$LOG_DIR/greet.log")"
grep -Fq 'npc '"$entity" "$LOG_DIR/greet.log" \
    || fail "Phase G: no selection stamped on Doc Mitchell"
echo "smoke[dt1-dialogue-layers]: Phase G PASS — quest-less activation greets"

# ── Phase F: the force-greet package opens without activation ─────
# End the G conversation first so the F assertion reads a fresh serial.
sleep 2
debug_command "dialogue.forcegreet $entity 0x000613BB" "$LOG_DIR/install.log" \
    || fail 'force-greet install unavailable'
grep -Fq 'force-greet installed' "$LOG_DIR/install.log" \
    || fail "force-greet install rejected: $(cat "$LOG_DIR/install.log")"

# The NPC walks to the player (KCC-stepped, walk speed) and the
# conversation opens with the package's topic. Poll for it.
deadline=$((SECONDS + TIMEOUT))
until grep -Fq "topic 0x0000C8" "$LOG_DIR/fg.status" 2>/dev/null; do
    (( SECONDS < deadline )) || fail "Phase F: the force-greet never opened the conversation"
    kill -0 "$engine_pid" 2>/dev/null || fail "engine exited during force-greet"
    sleep 1
    debug_command 'dialogue.status' "$LOG_DIR/fg.status" >/dev/null 2>&1 || true
done
echo "smoke[dt1-dialogue-layers]: Phase F PASS — PACK-driven conversation opened without activation"

stop_engine
echo "smoke[dt1-dialogue-layers]: PASS — G + F live; artifacts: $LOG_DIR"
