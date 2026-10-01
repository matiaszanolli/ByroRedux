#!/usr/bin/env bash
# P5 hardening gate 3 — save/reload across the P0 door route, both sides:
#
#   1. Player spawns at the P0 standoff, grounded; F5 quicksaves the
#      INTERIOR session (slot 0).
#   2. One bound-input E edge opens the door; the deferred orchestrator
#      applies the authored exterior transition (P0 route).
#   3. F9 quickload FROM the exterior session restores the INTERIOR save
#      cross-cell: pose returns to the quicksaved standoff.
#   4. The restored world is playable: the same door opens again, and the
#      exterior arrival repeats.
#   5. F5 outside (slot 2), engine.quit, relaunch --load 2: the EXTERIOR
#      session survives the process restart (grid + pose restored).
#
# Together with p5-save-restart (pose), p5-f5-f9-quicksave (F5/F9/quit/
# validation) and p5-quest-persistence (quest state), this closes the doc's
# "save/reload before and after door transitions" requirement.
set -euo pipefail
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/fixture.sh"
smoke_load_fixture p5-door-transition "$@"
smoke_require_fixture_fields \
    P0_CELL P0_CAMERA_POS P0_CAMERA_FORWARD P0_TARGET_KIND P0_PROMPT \
    P0_QUEUE_LOG P0_APPLIED_LOG P0_OUTCOME P1_ARRIVAL_GRID
smoke_require_data
cd "$SMOKE_ROOT_DIR"
PORT="${BYRO_DEBUG_PORT:-19874}"
TIMEOUT="${BYROREDUX_SMOKE_TIMEOUT:-300}"
LOG_DIR="$(mktemp -d /tmp/byro-p5-door.XXXXXX)"
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
    echo "smoke[p5-door-transition]: FAIL -- $*" >&2
    echo "artifacts: $LOG_DIR" >&2
    exit 1
}
if ss -ltn "sport = :$PORT" | grep -q LISTEN; then
    fail "port $PORT already occupied; refusing to attach to another session"
fi

cargo build --release --offline --quiet -p byroredux -p byro-dbg
debug_command() {
    timeout "$TIMEOUT" env BYRO_DEBUG_PORT="$PORT" target/release/byro-dbg >"$2" 2>&1 <<EOF
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
wait_file() {
    local file="$1" deadline=$((SECONDS + TIMEOUT))
    until [[ -s "$file" ]]; do
        kill -0 "$engine_pid" 2>/dev/null || fail "engine exited before $file was written"
        (( SECONDS < deadline )) || fail "timeout waiting for $file"
        sleep 0.25
    done
}
launch() {
    local phase="$1"
    shift
    env BYRO_DEBUG_PORT="$PORT" BYRO_DEBUG_SERVER=1 \
        BYROREDUX_SAVE_DIR="$LOG_DIR/saves" \
        BYROREDUX_SETTINGS_PATH="$LOG_DIR/settings.toml" \
        RUST_LOG="warn,byroredux::save_io=info,byroredux::loading_screen=info,byroredux::interaction=info,byroredux::cell_loader::transition=info,byroredux::app_step=info" \
        target/release/byroredux "${SMOKE_ENGINE_ARGS[@]}" \
        --cell "$P0_CELL" --player --radius 1 \
        --camera-pos "$P0_CAMERA_POS" --camera-forward "$P0_CAMERA_FORWARD" \
        --bench-frames 30 --bench-hold "$@" \
        >"$LOG_DIR/$phase.stdout" 2>"$LOG_DIR/$phase.stderr" &
    engine_pid=$!
    wait_log "$LOG_DIR/$phase.stderr" 'bench-hold:'
}
wait_grounded() {
    local output="$1" deadline=$((SECONDS + TIMEOUT))
    while true; do
        debug_command player.status "$output" || fail 'player status unavailable'
        if grep -Fq 'grounded=true' "$output" &&
           grep -Fq 'input_hold_frames_remaining=0' "$output"; then
            return
        fi
        (( SECONDS < deadline )) || fail 'character did not settle grounded'
        sleep 0.25
    done
}
pose() {
    sed -nE 's/.*body=\(([-0-9.]+), ([-0-9.]+), ([-0-9.]+)\).*/\1 \2 \3/p' "$1" | tail -1
}
pose_close() { # <fileA> <fileB> <max squared distance>
    awk -v a="$(pose "$1")" -v b="$(pose "$2")" -v max="$3" \
        'BEGIN { split(a,x); split(b,y); if(length(x)!=3 || length(y)!=3) exit 1; for(i=1;i<=3;i++) if((x[i]-y[i])^2>max) exit 1 }'
}
open_door() { # <label> — one E edge, wait for the authored exterior arrival
    local label="$1"
    debug_command 'interaction.status' "$LOG_DIR/prompt.$label.log" \
        || fail "interaction status unavailable ($label)"
    grep -Fq "$P0_PROMPT" "$LOG_DIR/prompt.$label.log" \
        || fail "door prompt missing before activation ($label)"
    debug_command 'input.press activate' "$LOG_DIR/press.$label.log" \
        || fail "activate press failed ($label)"
    grep -Fq 'input.press: queued action=Activate binding=E' "$LOG_DIR/press.$label.log" \
        || fail "E binding edge not queued ($label)"
    wait_log "$LOG_DIR/session.stderr" "$P0_APPLIED_LOG"
    # The transition drops the debug connection while the main thread
    # rebuilds the scene; wait for the server to accept connections again
    # and the arrival grid to show in player.status.
    local deadline=$((SECONDS + TIMEOUT))
    while true; do
        if debug_command player.status "$LOG_DIR/arrival.$label.status" 2>/dev/null &&
           grep -Fq "$P1_ARRIVAL_GRID" "$LOG_DIR/arrival.$label.status"; then
            return
        fi
        kill -0 "$engine_pid" 2>/dev/null || fail "engine exited after the door transition ($label)"
        (( SECONDS < deadline )) || fail "timeout waiting for exterior arrival ($label)"
        sleep 1
    done
}

launch session

# ── Gate 1 — quicksave the interior session at the standoff ────────────────
wait_grounded "$LOG_DIR/pose.inside.status"
debug_command 'input.press quicksave' "$LOG_DIR/f5-inside.log" || fail 'interior quicksave press failed'
grep -Fq 'action=Quicksave binding=F5' "$LOG_DIR/f5-inside.log" \
    || fail 'interior F5 edge not recognized'
wait_file "$LOG_DIR/saves/save_0.ess"

# ── Gate 2 — the authored door route to the exterior ───────────────────────
open_door outbound

# ── Gate 3 — F9 from the exterior restores the INTERIOR save cross-cell ────
pose_apart_check() { # <fileA> <fileB> <min squared distance> <label>
    awk -v a="$(pose "$1")" -v b="$(pose "$2")" -v min="$3" \
        'BEGIN { split(a,x); split(b,y); exit !(length(x)==3 && length(y)==3 && (x[1]-y[1])^2+(x[2]-y[2])^2+(x[3]-y[3])^2 >= min) }' \
        || fail "$4: exterior arrival pose is not distinct from the interior save"
}
pose_apart_check "$LOG_DIR/arrival.outbound.status" "$LOG_DIR/pose.inside.status" 10000 "outbound arrival"
debug_command 'input.press quickload' "$LOG_DIR/f9-crosscell.log" || fail 'cross-cell quickload press failed'
grep -Fq 'action=Quickload binding=F9' "$LOG_DIR/f9-crosscell.log" \
    || fail 'F9 edge not recognized for the cross-cell restore'
wait_log "$LOG_DIR/session.stderr" 'save load: restored player pose'
wait_grounded "$LOG_DIR/pose.restored-inside.status"
pose_close "$LOG_DIR/pose.restored-inside.status" "$LOG_DIR/pose.inside.status" 1 \
    || fail 'cross-cell quickload did not restore the interior standoff pose (≤1 unit)'

# ── Gate 4 — the restored world is playable: the door opens again ──────────
open_door restored

# ── Gate 5 — the exterior session survives a process restart ───────────────
# The ring cursor is in-memory: quicksave #1 wrote slot 0, quickload does not
# advance it, so this exterior quicksave lands in slot 1.
wait_grounded "$LOG_DIR/pose.outside.status"
debug_command 'input.press quicksave' "$LOG_DIR/f5-outside.log" || fail 'exterior quicksave press failed'
wait_file "$LOG_DIR/saves/save_1.ess"
debug_command 'engine.quit' "$LOG_DIR/quit.log" || fail 'engine.quit dispatch failed'
grep -Fq 'graceful shutdown requested' "$LOG_DIR/quit.log" \
    || fail 'engine.quit did not acknowledge'
set +e
wait "$engine_pid"
quit_status=$?
set -e
engine_pid=""
[[ "$quit_status" == 0 ]] || fail "graceful quit exited $quit_status (expected 0)"

launch restarted --load 1
wait_log "$LOG_DIR/restarted.stderr" 'save load: restored player pose'
wait_grounded "$LOG_DIR/pose.restarted.status"
grep -Fq "$P1_ARRIVAL_GRID" "$LOG_DIR/pose.restarted.status" \
    || fail "restarted session is not the exterior grid ($P1_ARRIVAL_GRID)"
pose_close "$LOG_DIR/pose.restarted.status" "$LOG_DIR/pose.outside.status" 4 \
    || fail 'restarted exterior pose differs from the saved exterior pose'

stop_engine
echo "smoke[p5-door-transition]: PASS -- F5 inside → door out → F9 cross-cell restore inside → door works again → F5 outside survives process restart"
echo "artifacts: $LOG_DIR"
