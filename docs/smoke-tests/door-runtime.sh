#!/usr/bin/env bash
# Owner-run live gate for the in-cell door runtime (PRD door-runtime, M66, #7059), Oblivion: in CheydinhalFightersGuild an
# unlocked door that authors `Open` and `Close` sequences opens on an activation, settles `open`, and closes on the next; a
# second door locked at level 50 with no key stays `closed` under an activation by a keyless activator (DRT-ACT-003). Both
# doors must report their authored sounds (DRT-SND-001; whether they are audible is not measurable from the console).
# `door.state` reads the state, `script.activate` raises the same `ActivateEvent` a keypress does.
# Requires a Vulkan device, display, and BYROREDUX_OBLIVION_DATA.
set -euo pipefail

GATE=door-runtime
DATA="${BYROREDUX_OBLIVION_DATA:-}"
if [[ -z "$DATA" || ! -f "$DATA/Oblivion.esm" || ! -f "$DATA/Oblivion - Meshes.bsa" || ! -f "$DATA/Oblivion - Textures - Compressed.bsa" ]]; then
    echo "smoke[$GATE]: SKIP -- missing BYROREDUX_OBLIVION_DATA (set it to the Oblivion Data directory)"
    exit 77
fi
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
DATA="$(cd "$DATA" && pwd)"
cd "$ROOT"
PORT="${BYRO_DEBUG_PORT:-19885}"
TIMEOUT="${BYROREDUX_SMOKE_TIMEOUT:-300}"
LOG_DIR="$(mktemp -d "/tmp/byro-$GATE.XXXXXX")"
engine_pid=""
stop_engine() {
    if [[ -n "$engine_pid" ]]; then
        kill -TERM "$engine_pid" 2>/dev/null || true
        wait "$engine_pid" 2>/dev/null || true
    fi
}
trap stop_engine EXIT
fail() { echo "smoke[$GATE]: FAIL -- $*; artifacts: $LOG_DIR" >&2; exit 1; }
if ss -ltn "sport = :$PORT" | grep -q LISTEN; then
    fail "port $PORT already occupied"
fi
if pgrep -x byroredux >/dev/null; then
    fail 'another engine instance is already running'
fi
cargo build --release --offline --quiet -p byroredux -p byro-dbg || fail 'build failed'
command() {
    timeout "$TIMEOUT" env BYRO_DEBUG_PORT="$PORT" "$ROOT/target/release/byro-dbg" >"$2" 2>&1 <<CMDS
$1
.quit
CMDS
    sed -i 's/\\n/\n/g' "$2"
}
env BYRO_DEBUG_PORT="$PORT" BYRO_DEBUG_SERVER=1 \
    BYROREDUX_SETTINGS_PATH="$LOG_DIR/settings.toml" \
    "$ROOT/target/release/byroredux" --esm "$DATA/Oblivion.esm" --cell CheydinhalFightersGuild \
    --bsa "$DATA/Oblivion - Meshes.bsa" --textures-bsa "$DATA/Oblivion - Textures - Compressed.bsa" \
    --player --bench-frames 30 --bench-hold >"$LOG_DIR/engine.log" 2>&1 &
engine_pid=$!
deadline=$((SECONDS + TIMEOUT))
until grep -Fq 'bench-hold:' "$LOG_DIR/engine.log"; do
    kill -0 "$engine_pid" 2>/dev/null || fail 'engine exited before bench-hold'
    (( SECONDS < deadline )) || fail 'timed out waiting for bench-hold'
    sleep 0.25
done


UNLOCKED=000954A7
LOCKED=00009470
status() { command 'door.state' "$LOG_DIR/status.log" || fail 'door.state failed'; }
row() { grep -F "ref $1" "$LOG_DIR/status.log" | head -1; }
entity_of() { row "$1" | sed -nE 's/.*door +([0-9]+) ref.*/\1/p'; }
state_of() { row "$1" | sed -nE 's/.*ref [0-9A-F]+  (open|closed|opening|closing)  .*/\1/p'; }
wait_for() { # <ref> <state>
    for _ in $(seq 1 30); do
        status
        [[ "$(state_of "$1")" == "$2" ]] && return 0
        sleep 0.5
    done
    return 1
}

status
free="$(entity_of $UNLOCKED)"; barred="$(entity_of $LOCKED)"
[[ -n "$free" && -n "$barred" ]] || fail "the guild's two doors are not both runtime doors: $(cat "$LOG_DIR/status.log")"
[[ "$(state_of $UNLOCKED)" == closed ]] || fail "the unlocked door does not start closed: $(row $UNLOCKED)"
row $LOCKED | grep -Fq 'locked (level 50' || fail "the barred door is not locked: $(row $LOCKED)"
for ref in $UNLOCKED $LOCKED; do
    row "$ref" | grep -Eq 'sounds open [0-9A-F]{8} close [0-9A-F]{8}' || fail "door $ref carries no authored open and close sounds: $(row "$ref")"
done

command "script.activate $free" "$LOG_DIR/open.log" || fail 'script.activate failed'
wait_for $UNLOCKED open || fail "the unlocked door did not open: $(row $UNLOCKED)"
command "script.activate $free" "$LOG_DIR/close.log" || fail 'script.activate failed'
wait_for $UNLOCKED closed || fail "the unlocked door did not close again: $(row $UNLOCKED)"

command "script.activate $barred" "$LOG_DIR/locked.log" || fail 'script.activate failed'
sleep 3
status
[[ "$(state_of $LOCKED)" == closed ]] || fail "the locked door opened for a keyless activator: $(row $LOCKED)"
grep -Eq 'panicked|PANIC' "$LOG_DIR/engine.log" && fail 'the engine panicked'
printf 'smoke[%s]: PASS -- door %s opened and closed, door %s stayed shut; artifacts: %s\n' "$GATE" "$free" "$barred" "$LOG_DIR"
