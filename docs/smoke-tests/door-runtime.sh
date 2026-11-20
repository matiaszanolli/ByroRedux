#!/usr/bin/env bash
# Owner-run live gate for the in-cell door runtime (PRD door-runtime, M66, #7059): `door-runtime.sh [oblivion|fnv]`.
# Each leg boots a cell with an unlocked door that authors `Open` and `Close` sequences and a locked door with no key (or a
# keyed one). The unlocked door changes state on an activation, settles, and returns on the next (it may start `open`: Open by Default); the locked door stays
# `closed` under an activation by a keyless activator (DRT-ACT-003); both report their authored sounds (DRT-SND-001;
# whether they are audible is not measurable from the console). `door.state` reads the state, `script.activate` raises the
# same `ActivateEvent` a keypress does.
#   oblivion: CheydinhalFightersGuild (a level-50 lock, no key), then TempleMothCatacombs02, where the level-10 door that
#             ends the Moth priest's route (00094D23) stays shut without its key (0009DAC6) and opens with it.
#   fnv:      JacobstownLodge (a level-75 lock, no key).
# Requires a Vulkan device, display, and the game's data (BYROREDUX_OBLIVION_DATA, BYROREDUX_FNV_DATA).
set -euo pipefail

GATE=door-runtime
GAME="${1:-oblivion}"
case "$GAME" in
    oblivion)
        DATA="${BYROREDUX_OBLIVION_DATA:-}"; MASTER=Oblivion.esm; MESHES="Oblivion - Meshes.bsa"; TEXTURES="Oblivion - Textures - Compressed.bsa"
        VAR=BYROREDUX_OBLIVION_DATA ;;
    fnv)
        DATA="${BYROREDUX_FNV_DATA:-/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data}"; MASTER=FalloutNV.esm
        MESHES="Fallout - Meshes.bsa"; TEXTURES="Fallout - Textures.bsa"; VAR=BYROREDUX_FNV_DATA ;;
    *) echo "smoke[$GATE]: usage: door-runtime.sh [oblivion|fnv]" >&2; exit 2 ;;
esac
if [[ -z "$DATA" || ! -f "$DATA/$MASTER" || ! -f "$DATA/$MESHES" || ! -f "$DATA/$TEXTURES" ]]; then
    echo "smoke[$GATE]: SKIP -- missing $VAR (set it to the $GAME Data directory)"
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
start_engine() { # <cell>
    stop_engine
    : >"$LOG_DIR/engine.log"
    env BYRO_DEBUG_PORT="$PORT" BYRO_DEBUG_SERVER=1 \
        BYROREDUX_SETTINGS_PATH="$LOG_DIR/settings.toml" \
        "$ROOT/target/release/byroredux" --esm "$DATA/$MASTER" --cell "$1" \
        --bsa "$DATA/$MESHES" --textures-bsa "$DATA/$TEXTURES" \
        --player --bench-frames 30 --bench-hold >>"$LOG_DIR/engine.log" 2>&1 &
    engine_pid=$!
    local deadline=$((SECONDS + TIMEOUT))
    until grep -Fq 'bench-hold:' "$LOG_DIR/engine.log"; do
        kill -0 "$engine_pid" 2>/dev/null || fail "engine exited before bench-hold ($1)"
        (( SECONDS < deadline )) || fail "timed out waiting for bench-hold ($1)"
        sleep 0.25
    done
}

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
# A door reads at rest for the frame between the toggle and the clip's start, so a state read straight after an activation can
# precede the motion: let the clip play out (the longest authored one is a second or two) before the next activation.
settle() { sleep 3; }
activate() { command "script.activate $1" "$LOG_DIR/activate.log" || fail "script.activate $1 failed"; }

# A cell with a free door and a barred one: the free one opens and closes, the barred one stays shut.
free_and_barred() { # <cell> <free ref> <barred ref> <lock text>
    start_engine "$1"
    status
    local free barred
    free="$(entity_of "$2")"; barred="$(entity_of "$3")"
    [[ -n "$free" && -n "$barred" ]] || fail "$1: the two doors are not both runtime doors: $(cat "$LOG_DIR/status.log")"
    # A door that is `Open by Default` spawns open (DRT-STATE-003): the free door starts in its default state.
    local start other
    start="$(state_of "$2")"
    case "$start" in
        closed) other=open ;;
        open) other=closed ;;
        *) fail "$1: the free door does not start at rest: $(row "$2")" ;;
    esac
    row "$2" | grep -Fq "default $start" || fail "$1: the free door does not start in its default state: $(row "$2")"
    row "$3" | grep -Fq "$4" || fail "$1: the barred door is not locked as expected: $(row "$3")"
    local ref
    for ref in "$2" "$3"; do
        row "$ref" | grep -Eq 'sounds open [0-9A-F]{8} close [0-9A-F]{8}' || fail "$1: door $ref carries no authored open and close sounds: $(row "$ref")"
    done
    activate "$free"; wait_for "$2" "$other" || fail "$1: the free door did not go $other: $(row "$2")"; settle
    activate "$free"; wait_for "$2" "$start" || fail "$1: the free door did not return to $start: $(row "$2")"
    activate "$barred"; sleep 3; status
    [[ "$(state_of "$3")" == closed ]] || fail "$1: the locked door opened for a keyless activator: $(row "$3")"
    printf 'smoke[%s]: %s: door %s went %s and back to %s, door %s stayed shut\n' "$GATE" "$1" "$free" "$other" "$start" "$barred"
}

case "$GAME" in
    oblivion)
        free_and_barred CheydinhalFightersGuild 000954A7 00009470 'locked (level 50'
        # The keyed door: shut without the key, open with it (DRT-ACT-003).
        start_engine TempleMothCatacombs02
        status
        door="$(entity_of 00094D23)"
        [[ -n "$door" ]] || fail "TempleMothCatacombs02: the priest's door is not a runtime door: $(cat "$LOG_DIR/status.log")"
        row 00094D23 | grep -Fq 'key 0009DAC6' || fail "the priest's door does not name its key: $(row 00094D23)"
        activate "$door"; sleep 3; status
        [[ "$(state_of 00094D23)" == closed ]] || fail "the keyed door opened without its key: $(row 00094D23)"
        command 'inv.add 0x0009DAC6 1' "$LOG_DIR/key.log" || fail 'inv.add failed'
        activate "$door"
        wait_for 00094D23 open || fail "the keyed door did not open for the key's carrier: $(row 00094D23)"
        printf 'smoke[%s]: TempleMothCatacombs02: door %s stayed shut without the key and opened with it\n' "$GATE" "$door"
        ;;
    fnv)
        free_and_barred JacobstownLodge 0013F713 0013D847 'locked (level 75'
        ;;
esac
grep -Eq 'panicked|PANIC' "$LOG_DIR/engine.log" && fail 'the engine panicked'
printf 'smoke[%s]: PASS -- %s; artifacts: %s\n' "$GATE" "$GAME" "$LOG_DIR"
