#!/usr/bin/env bash
# Playable-slice P0 close-out smoke: exercise a real interior door through
# the production interaction path.
#
# Game-parameterised (#3039): every cell / pose / destination literal comes
# from `fixtures/<game>.env`. Pass the game as the first argument (or set
# `BYROREDUX_SMOKE_GAME`); default `skyrim_se`.
#
#   docs/smoke-tests/p0-door-interaction.sh              # Skyrim SE
#   docs/smoke-tests/p0-door-interaction.sh fnv          # Fallout New Vegas
#
# The camera pose is a stable fly-camera preflight aimed at the fixture's
# authored XTEL door. FlyCam keeps the targeting fixture deterministic;
# character traversal across the threshold belongs to P1. `input.press
# activate` queues one KeyE pulse into the normal ActionBindings/ActionState
# edge path -- it does not call activation or cell transition code directly.
#
# Gate:
#   prompt -> KeyE edge -> ActivateEvent -> persistent destination lookup
#   -> deferred interior unload -> arrival at the authored destination.

set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/fixture.sh"
smoke_load_fixture p0-door-interaction "$@"
smoke_require_fixture_fields \
    P0_CELL P0_CAMERA_POS P0_CAMERA_FORWARD P0_TARGET_KIND P0_PROMPT \
    P0_QUEUE_LOG P0_APPLIED_LOG P0_OUTCOME P0_ENTITY_FLOOR

ROOT_DIR="$SMOKE_ROOT_DIR"
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

smoke_require_data

echo "================================================================"
echo "  smoke[p0-door-interaction]: $FIXTURE_LABEL -- $P0_HEADLINE"
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
    "${SMOKE_ENGINE_ARGS[@]}" \
    --cell "$P0_CELL" \
    --fly \
    --camera-pos "$P0_CAMERA_POS" \
    --camera-forward "$P0_CAMERA_FORWARD" \
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

require_in "$preflight_log" "$P0_TARGET_KIND" "camera-forward target is a real XTEL door"
require_in "$preflight_log" "$P0_PROMPT" "native interaction prompt is present"
require_in "$preflight_log" "activations=0" "fixture starts without a stale activation edge"

BYRO_DEBUG_PORT="$PORT" cargo run --release --quiet -p byro-dbg <<'EOF' >"$press_log" 2>&1 || true
input.press activate
.quit
EOF
require_in "$press_log" \
    "input.press: queued action=Activate binding=E" \
    "smoke input entered through the normal KeyE binding"

deadline=$(( $(date +%s) + TIMEOUT ))
while ! grep -F "Cell transition applied:" "$engine_stderr" \
    | grep -Fq "$P0_APPLIED_LOG"; do
    if [[ $(date +%s) -gt $deadline ]]; then
        echo "smoke[p0-door-interaction]: FAIL -- timeout waiting for the authored transition"
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
# rebuilds the destination scene. Reconnect after the applied log and inspect
# the retained InteractionTrace.
if (( hard_fail == 0 )); then
    BYRO_DEBUG_PORT="$PORT" cargo run --release --quiet -p byro-dbg <<'EOF' >"$arrival_log" 2>&1
interaction.status
.quit
EOF
fi

require_in "$engine_stderr" \
    "$P0_QUEUE_LOG" \
    "door activation queued the persistent destination"
require_in "$engine_stderr" \
    "$P0_APPLIED_LOG" \
    "deferred orchestrator applied the transition"
require_in "$arrival_log" "activations=1" "exactly one Activate edge was consumed"
require_in "$arrival_log" "event_emitted=true" "canonical ActivateEvent was emitted"
require_in "$arrival_log" \
    "$P0_OUTCOME" \
    "post-transition trace retained the successful outcome"

bench_line="$(grep '^bench:' "$engine_stdout" | tail -1 || true)"
if [[ -z "$bench_line" ]]; then
    echo "smoke[p0-door-interaction]: FAIL -- no bench summary"
    hard_fail=1
else
    echo "$bench_line"
    entities="$(echo "$bench_line" | grep -oE 'entities=[0-9]+' | head -1 | cut -d= -f2)"
    : "${entities:=0}"
    if (( entities < P0_ENTITY_FLOOR )); then
        echo "smoke[p0-door-interaction]: FAIL -- source entities=$entities < floor $P0_ENTITY_FLOOR"
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

echo "smoke[p0-door-interaction]: PASS -- prompt -> E -> ActivateEvent -> $P0_HEADLINE"
