#!/usr/bin/env bash
# P5 hardening gate 1 — the native quicksave/quickload route through bound
# input, in one live process, plus the graceful-quit and Vulkan-validation
# gates. Companion to `p5-save-restart.sh` (which proves the pose survives a
# process restart through console `save`/`--load`); this gate proves:
#
#   1. `input.press quicksave` delivers one F5 binding edge into the same
#      canonical `PlayerSaveAction` queue the winit key handler uses
#      (#3113), and the deferred executor commits a validated ring slot.
#   2. A second quicksave after real bound-input displacement lands in the
#      next ring slot.
#   3. `input.press quickload` (F9) restores the newest slot IN-PROCESS:
#      the body returns to the quicksaved pose — not the pre-save spawn,
#      not the post-save moved pose.
#   4. `engine.quit` performs the shared orderly shutdown (streaming unload,
#      renderer teardown) and the process exits 0 without panic or hang.
#   5. The whole session runs under `BYRO_VALIDATION=1` and the Khronos
#      validation layer reports zero errors (debug Vulkan validation gate).
#
# Not covered here (kept to their own gates): inventory/equipment and death
# markers across reload (p2-melee-core), quest/objective state (p4 route +
# p5-quest-persistence), door-transition saves (p5-door-transition), the
# 30-minute soak, and process-restart restore (p5-save-restart).
set -euo pipefail
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/fixture.sh"
smoke_load_fixture p5-f5-f9-quicksave "$@"
smoke_require_fixture_fields P1_CELL P1_CAMERA_POS P1_CAMERA_FORWARD
smoke_require_data
cd "$SMOKE_ROOT_DIR"
PORT="${BYRO_DEBUG_PORT:-19877}"
TIMEOUT="${BYROREDUX_SMOKE_TIMEOUT:-180}"
LOG_DIR="$(mktemp -d /tmp/byro-p5-f5-f9.XXXXXX)"
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
    echo "smoke[p5-f5-f9-quicksave]: FAIL -- $*" >&2
    echo "artifacts: $LOG_DIR" >&2
    exit 1
}
if ss -ltn "sport = :$PORT" | grep -q LISTEN; then
    fail "port $PORT already occupied; refusing to attach to another session"
fi
# The validation leg is part of this gate: without the Khronos layer
# installed the run would silently exercise no validation at all. Captured
# to a variable first — `vulkaninfo` can exit non-zero after printing its
# layer table, which under `pipefail` would flip this check the wrong way.
if ! command -v vulkaninfo >/dev/null 2>&1; then
    echo "smoke[p5-f5-f9-quicksave]: SKIP 77 -- vulkaninfo unavailable; cannot prove the validation layer" >&2
    exit 77
fi
vulkan_layer_table="$(vulkaninfo --summary 2>/dev/null || true)"
if ! grep -q "VK_LAYER_KHRONOS_validation" <<<"$vulkan_layer_table"; then
    echo "smoke[p5-f5-f9-quicksave]: SKIP 77 -- Vulkan Khronos validation layer not available" >&2
    exit 77
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
        BYRO_VALIDATION=1 \
        RUST_LOG=warn,byroredux::save_io=info,byroredux::loading_screen=info,byroredux::app_events=info \
        target/release/byroredux "${SMOKE_ENGINE_ARGS[@]}" \
        --cell "$P1_CELL" --player --radius 1 \
        --camera-pos "$P1_CAMERA_POS" --camera-forward "$P1_CAMERA_FORWARD" \
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
pose_apart() { # <fileA> <fileB> <min squared distance>
    awk -v a="$(pose "$1")" -v b="$(pose "$2")" -v min="$3" \
        'BEGIN { split(a,x); split(b,y); exit !(length(x)==3 && length(y)==3 && (x[1]-y[1])^2+(x[2]-y[2])^2+(x[3]-y[3])^2 >= min) }'
}
pose_close() { # <fileA> <fileB> <max squared distance>
    awk -v a="$(pose "$1")" -v b="$(pose "$2")" -v max="$3" \
        'BEGIN { split(a,x); split(b,y); if(length(x)!=3 || length(y)!=3) exit 1; for(i=1;i<=3;i++) if((x[i]-y[i])^2>max) exit 1 }'
}
move_at_least() { # <label> <baseline-status-file> <hold-frames> [action]
    local label="$1" baseline="$2" frames="$3" action="${4:-backward}"
    debug_command "input.hold $action $frames" "$LOG_DIR/move.$label.log" \
        || fail "input hold ($label) failed"
    grep -Fq "for $frames frames" "$LOG_DIR/move.$label.log" \
        || fail "input hold ($label) not queued"
    local deadline=$((SECONDS + TIMEOUT))
    while true; do
        wait_grounded "$LOG_DIR/pose.$label.status"
        if pose_apart "$LOG_DIR/pose.$label.status" "$baseline" 100; then
            return
        fi
        (( SECONDS < deadline )) || fail "character did not move ≥10 units before $label"
        sleep 0.25
    done
}

launch session
wait_grounded "$LOG_DIR/pose0.status"

# ── Gate 1 — F5 quicksave commits a validated ring slot ────────────────────
# The quicksave ring is 0-based and resumes past the newest on-disk slot
# (SAVE-D3-02); the isolated save dir starts empty, so the first two F5
# presses land in slots 0 and 1.
debug_command 'input.press quicksave' "$LOG_DIR/press1.log" || fail 'quicksave press failed'
grep -Fq 'input.press: queued action=Quicksave binding=F5; player save action deferred' \
    "$LOG_DIR/press1.log" || fail 'F5 binding edge was not queued through the canonical save path'
wait_file "$LOG_DIR/saves/save_0.ess"
debug_command 'save.info 0' "$LOG_DIR/saveinfo0.log" || fail 'save.info 0 failed'
grep -Fq 'slot 0: VALID' "$LOG_DIR/saveinfo0.log" || fail 'quicksave slot 0 did not validate'

# ── Gate 2 — a second quicksave after real displacement lands in slot 1 ────
move_at_least move1 "$LOG_DIR/pose0.status" 120
debug_command 'input.press quicksave' "$LOG_DIR/press2.log" || fail 'second quicksave press failed'
grep -Fq 'action=Quicksave binding=F5' "$LOG_DIR/press2.log" \
    || fail 'second F5 press was not recognized'
wait_file "$LOG_DIR/saves/save_1.ess"
cp "$LOG_DIR/pose.move1.status" "$LOG_DIR/poseA.status"

# ── Gate 3 — F9 quickload restores the newest slot in-process ───────────────
# move2 retraces move1 in reverse (forward, back toward the door) — a second
# backward hold runs the character into the saloon's back wall and stalls.
move_at_least move2 "$LOG_DIR/poseA.status" 120 forward
pose_apart "$LOG_DIR/pose.move2.status" "$LOG_DIR/poseA.status" 100 \
    || fail 'post-save movement did not displace the body (nothing to restore)'
debug_command 'input.press quickload' "$LOG_DIR/press9.log" || fail 'quickload press failed'
grep -Fq 'input.press: queued action=Quickload binding=F9; player save action deferred' \
    "$LOG_DIR/press9.log" || fail 'F9 binding edge was not queued through the canonical load path'
wait_log "$LOG_DIR/session.stderr" 'save load: restored player pose'
if [[ "${P1_EXPECT_LOADING_SCREEN:-0}" == 1 ]]; then
    wait_log "$LOG_DIR/session.stderr" 'loading.screen: dismissed after destination frame'
fi
wait_grounded "$LOG_DIR/pose.restored.status"
pose_close "$LOG_DIR/pose.restored.status" "$LOG_DIR/poseA.status" 1 \
    || fail 'quickload did not restore the quicksaved pose (≤1 unit)'
pose_apart "$LOG_DIR/pose.restored.status" "$LOG_DIR/pose.move2.status" 100 \
    || fail 'restored pose equals the pre-load moved pose (load was a no-op)'

# ── Gate 4 — engine.quit performs the orderly shutdown, exit 0 ──────────────
debug_command 'engine.quit' "$LOG_DIR/quit.log" || fail 'engine.quit dispatch failed'
grep -Fq 'graceful shutdown requested' "$LOG_DIR/quit.log" \
    || fail 'engine.quit did not acknowledge the shutdown request'
quit_deadline=$((SECONDS + TIMEOUT))
while kill -0 "$engine_pid" 2>/dev/null; do
    (( SECONDS < quit_deadline )) || fail "engine did not exit within ${TIMEOUT}s of engine.quit"
    sleep 0.25
done
set +e
wait "$engine_pid"
quit_status=$?
set -e
engine_pid=""
[[ "$quit_status" == 0 ]] || fail "graceful quit exited $quit_status (expected 0)"
grep -Fq 'Shutdown requested' "$LOG_DIR/session.stderr" \
    || fail 'orderly shutdown log line missing (teardown path not taken?)'
# The streaming-unload stage ('Streaming shutdown: unloading N streamed
# cells') only logs for exterior sessions — an interior cell has no
# `WorldStreamingState`, so its absence here is correct, not a skipped
# teardown. Exit 0 + no validation errors + no panic below carry the rest.

# ── Gate 5 — validation-clean session ───────────────────────────────────────
grep -Fq 'Vulkan VALIDATION ENABLED' "$LOG_DIR/session.stderr" \
    || fail 'validation leg did not run (BYRO_VALIDATION ignored?)'
if grep -E 'Validation Error|VUID-' "$LOG_DIR/session.stdout" "$LOG_DIR/session.stderr" >/dev/null; then
    grep -E 'Validation Error|VUID-' "$LOG_DIR/session.stdout" "$LOG_DIR/session.stderr" | head -5 >&2
    fail 'Vulkan validation errors during the session (see lines above)'
fi
if grep -Fq 'panicked at' "$LOG_DIR/session.stderr"; then
    fail 'engine panicked during the session'
fi

echo "smoke[p5-f5-f9-quicksave]: PASS -- F5 quicksave ring slots 0+1, F9 in-process pose restore, graceful quit (exit 0), validation clean"
echo "artifacts: $LOG_DIR"
