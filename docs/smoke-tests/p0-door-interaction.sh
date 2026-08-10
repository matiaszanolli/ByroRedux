#!/usr/bin/env bash
# Playable-slice P0 close-out smoke: exercise a real Skyrim interior door
# through the production interaction path.
#
# Fixture: WhiterunBanneredMare's vanilla XTEL exit. The camera pose is a
# stable fly-camera preflight aimed at that authored door. FlyCam keeps the
# targeting fixture deterministic; character traversal across the threshold
# belongs to P1. `input.press activate` queues one KeyE pulse into the normal
# ActionBindings/ActionState edge path -- it does not call activation or cell
# transition code directly.
#
# Gate:
#   prompt -> KeyE edge -> ActivateEvent -> persistent exterior destination
#   lookup -> deferred interior unload -> WhiterunWorld (6,-2) arrival.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SKYRIM_DATA="${BYROREDUX_SKYRIM_DATA:-/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data}"
PORT="${BYRO_DEBUG_PORT:-9876}"
BENCH_FRAMES="${BYROREDUX_SMOKE_FRAMES:-30}"
TIMEOUT="${BYROREDUX_SMOKE_TIMEOUT:-240}"

LOG_DIR="$(mktemp -d)"
engine_pid=""
cleanup() {
    if [[ -n "$engine_pid" ]]; then
        kill -TERM "$engine_pid" 2>/dev/null || true
        wait "$engine_pid" 2>/dev/null || true
    fi
    rm -rf "$LOG_DIR"
}
trap cleanup EXIT

for required in \
    "$SKYRIM_DATA/Skyrim.esm" \
    "$SKYRIM_DATA/Skyrim - Meshes0.bsa" \
    "$SKYRIM_DATA/Skyrim - Textures0.bsa" \
    "$SKYRIM_DATA/Skyrim - Misc.bsa"; do
    if [[ ! -f "$required" ]]; then
        echo "smoke[p0-door-interaction]: SKIP -- missing $required"
        exit 0
    fi
done

echo "================================================================"
echo "  smoke[p0-door-interaction]: Bannered Mare -> WhiterunWorld"
echo "================================================================"

engine_stdout="$LOG_DIR/engine.stdout"
engine_stderr="$LOG_DIR/engine.stderr"
preflight_log="$LOG_DIR/preflight.debug.log"
press_log="$LOG_DIR/press.debug.log"
arrival_log="$LOG_DIR/arrival.debug.log"

cd "$ROOT_DIR"
BYRO_DEBUG_PORT="$PORT" \
RUST_LOG="warn,byroredux::interaction=info,byroredux::cell_loader::transition=info,byroredux::app_step=info" \
cargo run --release --quiet -- \
    --esm "$SKYRIM_DATA/Skyrim.esm" \
    --cell WhiterunBanneredMare \
    --bsa "$SKYRIM_DATA/Skyrim - Meshes0.bsa" \
    --textures-bsa "$SKYRIM_DATA/Skyrim - Textures0.bsa" \
    --scripts-bsa "$SKYRIM_DATA/Skyrim - Misc.bsa" \
    --fly \
    --camera-pos "-116,226,982" \
    --camera-forward "0,-0.29,0.96" \
    --bench-frames "$BENCH_FRAMES" \
    --bench-hold \
    >"$engine_stdout" 2>"$engine_stderr" &
engine_pid=$!

deadline=$(( $(date +%s) + TIMEOUT ))
while ! grep -q '^bench-hold:' "$engine_stderr" 2>/dev/null; do
    if [[ $(date +%s) -gt $deadline ]]; then
        echo "smoke[p0-door-interaction]: FAIL -- timeout waiting for bench-hold"
        tail -40 "$engine_stderr" || true
        exit 1
    fi
    if ! kill -0 "$engine_pid" 2>/dev/null; then
        echo "smoke[p0-door-interaction]: FAIL -- engine exited before bench-hold"
        tail -40 "$engine_stderr" || true
        exit 1
    fi
    sleep 0.5
done

BYRO_DEBUG_PORT="$PORT" cargo run --release --quiet -p byro-dbg <<'EOF' >"$preflight_log" 2>&1
interaction.status
.quit
EOF

hard_fail=0
require_in() {
    local file="$1"
    local pattern="$2"
    local description="$3"
    if grep -Fq "$pattern" "$file"; then
        echo "smoke[p0-door-interaction]: PASS -- $description"
    else
        echo "smoke[p0-door-interaction]: FAIL -- $description (missing '$pattern')"
        hard_fail=1
    fi
}

require_in "$preflight_log" "kind=Door" "camera-forward target is a real XTEL door"
require_in "$preflight_log" "prompt=[E] Open" "native interaction prompt is present"
require_in "$preflight_log" "activations=0" "fixture starts without a stale activation edge"

BYRO_DEBUG_PORT="$PORT" cargo run --release --quiet -p byro-dbg <<'EOF' >"$press_log" 2>&1 || true
input.press activate
.quit
EOF
require_in "$press_log" \
    "input.press: queued KeyE through the normal Activate binding" \
    "smoke input entered through the normal KeyE binding"

deadline=$(( $(date +%s) + TIMEOUT ))
while ! grep -F "Cell transition applied:" "$engine_stderr" \
    | grep -Fq "exterior 'whiterunworld' (6,-2) at world"; do
    if [[ $(date +%s) -gt $deadline ]]; then
        echo "smoke[p0-door-interaction]: FAIL -- timeout waiting for exterior transition"
        hard_fail=1
        break
    fi
    if ! kill -0 "$engine_pid" 2>/dev/null; then
        echo "smoke[p0-door-interaction]: FAIL -- engine exited during transition"
        hard_fail=1
        break
    fi
    sleep 0.5
done

# The transition temporarily drops the debug connection while the main thread
# rebuilds the exterior scene. Reconnect after the applied log and inspect the
# retained InteractionTrace.
if (( hard_fail == 0 )); then
    BYRO_DEBUG_PORT="$PORT" cargo run --release --quiet -p byro-dbg <<'EOF' >"$arrival_log" 2>&1
interaction.status
.quit
EOF
fi

require_in "$engine_stderr" \
    "activated; queued exterior 'whiterunworld' (6,-2)" \
    "door activation queued the persistent exterior destination"
require_in "$engine_stderr" \
    "exterior 'whiterunworld' (6,-2) at world" \
    "deferred orchestrator applied the exterior transition"
require_in "$arrival_log" "activations=1" "exactly one Activate edge was consumed"
require_in "$arrival_log" "event_emitted=true" "canonical ActivateEvent was emitted"
require_in "$arrival_log" \
    "outcome=ActivateEvent emitted; queued exterior 'whiterunworld' (6,-2)" \
    "post-transition trace retained the successful outcome"

bench_line="$(grep '^bench:' "$engine_stdout" | tail -1 || true)"
if [[ -z "$bench_line" ]]; then
    echo "smoke[p0-door-interaction]: FAIL -- no bench summary"
    hard_fail=1
else
    echo "$bench_line"
    entities="$(echo "$bench_line" | grep -oE 'entities=[0-9]+' | head -1 | cut -d= -f2)"
    : "${entities:=0}"
    if (( entities < 3000 )); then
        echo "smoke[p0-door-interaction]: FAIL -- source entities=$entities < floor 3000"
        hard_fail=1
    else
        echo "smoke[p0-door-interaction]: PASS -- source cell populated ($entities entities)"
    fi
fi

if (( hard_fail != 0 )); then
    echo "-- preflight debug output ---------------------------------------"
    cat "$preflight_log" 2>/dev/null || true
    echo "-- press debug output -------------------------------------------"
    cat "$press_log" 2>/dev/null || true
    echo "-- arrival debug output -----------------------------------------"
    cat "$arrival_log" 2>/dev/null || true
    echo "-- engine transition log ---------------------------------------"
    grep -E 'interaction:|Transition:|Cell transition' "$engine_stderr" || true
    exit "$hard_fail"
fi

echo "smoke[p0-door-interaction]: PASS -- prompt -> E -> ActivateEvent -> WhiterunWorld"
