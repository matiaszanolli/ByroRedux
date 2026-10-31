#!/usr/bin/env bash
# M43 gate — Skyrim's NVPE, ARRT and ESJA producers, live on the real master.
#
#   1. Boot the Skyrim SE interior `WhiterunDragonsreachBasement` — Whiterun's jail cell, so the jail move is
#      in place and the only cell swap is the escape out to the exterior (an exterior-to-jail swap device-loses
#      the GPU on the dev machine, whatever this code does; see the session notes) — with a player.
#   2. `word.teach` a word of power (HowlCallOfTheWild3): the engine logs `word of power ... obtained; raising
#      NVPE`; teaching it again is a no-op.
#   3. `jail.send` Whiterun's crime faction (CrimeFactionWhiterun, 0x267EA) as a guard's dialogue does
#      (`akSpeaker.GetCrimeFaction().SendPlayerToJail()`): `SendPlayerToJail: ... raising ARRT`, and the player
#      is sent to the faction's jail cell (the jail whose exit is the faction's Exterior Jail Marker).
#   4. Once inside, `jail.escape` walks the player out to the exit without serving: `jail escape detected;
#      raising ESJA`.
set -euo pipefail

PORT="${BYRO_DEBUG_PORT:-19888}"
TIMEOUT="${BYROREDUX_SMOKE_TIMEOUT:-300}"
CELL="${BYRO_SMOKE_JAIL_CELL:-WhiterunDragonsreachBasement}"
WORD="${BYRO_SMOKE_WORD:-0x000E5F67}"
FACTION="${BYRO_SMOKE_CRIME_FACTION:-0x000267EA}"
LOG_DIR="$(mktemp -d /tmp/byro-m43-skyrim-actions.XXXXXX)"
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
    echo "smoke[m43-skyrim-actions]: FAIL -- $*" >&2
    echo "artifacts: $LOG_DIR" >&2
    exit 1
}

# #5427 — SKIP != PASS: a runner without the game's data measures nothing.
DATA="${BYROREDUX_SKYRIMSE_DATA:-/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data}"
for f in "Skyrim.esm" "Skyrim - Meshes0.bsa" "Skyrim - Textures0.bsa"; do
    if [ ! -f "$DATA/$f" ]; then
        echo "smoke[m43-skyrim-actions]: SKIP -- missing $DATA/$f"
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
    RUST_LOG="warn,byroredux_scripting::words_of_power=info,byroredux_scripting::jail=info" \
    target/release/byroredux \
    --game skyrim_se --cell "$CELL" \
    --player --bench-frames 30 --bench-hold \
    >"$LOG_DIR/session.stdout" 2>"$LOG_DIR/session.stderr" &
engine_pid=$!

wait_log "$LOG_DIR/session.stderr" 'bench-hold:'
sleep 8

# A scripted MoveTo is ignored while the launch's explicit start location is pinned; the first key press
# releases the pin (ExplicitStartPin), as it would for a player; `start.release` does the same.
debug_command "start.release" "$LOG_DIR/pin.log" || fail "start.release unavailable"
sleep 2

# NVPE
debug_command "word.teach $WORD" "$LOG_DIR/word1.log" || fail "word.teach unavailable"
grep -Fq 'obtained' "$LOG_DIR/word1.log" || fail "the word was not obtained: $(sed -n '1,4p' "$LOG_DIR/word1.log")"
wait_log "$LOG_DIR/session.stderr" 'word of power [0-9A-F]+ obtained; raising NVPE'
debug_command "word.teach $WORD" "$LOG_DIR/word2.log" || fail "word.teach unavailable"
grep -Fq 'already known' "$LOG_DIR/word2.log" || fail "a known word should be a no-op: $(sed -n '1,4p' "$LOG_DIR/word2.log")"

# ARRT (+ the jail itself)
debug_command "jail.send $FACTION" "$LOG_DIR/jail.log" || fail "jail.send unavailable"
wait_log "$LOG_DIR/session.stderr" 'SendPlayerToJail: faction 000267EA, real=true; raising ARRT'
grep -Fq 'sent to a cell' "$LOG_DIR/jail.log" \
    || fail "the player was not sent to Whiterun's jail cell: $(sed -n '1,5p' "$LOG_DIR/jail.log")"
sleep 6

# ESJA
debug_command "jail.escape" "$LOG_DIR/escape.log" || fail "jail.escape unavailable"
grep -Fq 'moving the player' "$LOG_DIR/escape.log" || fail "no sentence to escape: $(sed -n '1,5p' "$LOG_DIR/escape.log")"
wait_log "$LOG_DIR/session.stderr" 'jail escape detected; raising ESJA'

if grep -Fq 'panicked' "$LOG_DIR/session.stderr"; then
    fail "the engine panicked: $(grep -F 'panicked' "$LOG_DIR/session.stderr" | head -1)"
fi

stop_engine
echo "smoke[m43-skyrim-actions]: PASS -- NVPE (word taught), ARRT (Whiterun's crime faction sent the player to jail)"
echo "and ESJA (the unserved sentence was escaped) were raised live."
echo "artifacts: $LOG_DIR"
