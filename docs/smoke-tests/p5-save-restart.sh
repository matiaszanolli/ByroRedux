#!/usr/bin/env bash
# Persistence baseline, not full P5 closure: character pose survives an OS
# process restart. Save commands share the production serializer; this does
# not verify native F5/F9 delivery, inventory/quests, or the 30-minute soak.
set -euo pipefail
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/fixture.sh"
smoke_load_fixture p5-save-restart "$@"
smoke_require_fixture_fields P1_CELL P1_CAMERA_POS P1_CAMERA_FORWARD
smoke_require_data
cd "$SMOKE_ROOT_DIR"
PORT="${BYRO_DEBUG_PORT:-19876}"
TIMEOUT="${BYROREDUX_SMOKE_TIMEOUT:-180}"
LOG_DIR="$(mktemp -d /tmp/byro-p5-save-restart.XXXXXX)"
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
    echo "smoke[p5-save-restart]: FAIL -- $*" >&2
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
launch() {
    local phase="$1"
    shift
    env BYRO_DEBUG_PORT="$PORT" BYROREDUX_SAVE_DIR="$LOG_DIR/saves" \
        RUST_LOG=warn,byroredux::save_io=info,byroredux::loading_screen=info \
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
    sed -nE 's/.*body=\(([-0-9.]+), ([-0-9.]+), ([-0-9.]+)\).*/\1 \2 \3/p' "$1"
}
launch initial
wait_grounded "$LOG_DIR/before.status"
debug_command 'input.hold backward 120' "$LOG_DIR/move.log" || fail 'input hold failed'
grep -Fq 'for 120 frames' "$LOG_DIR/move.log" || fail 'input hold not queued'
# Require observed displacement as well as completion, so a stale pre-hold
# zero countdown cannot pass. A saved default spawn would not prove restore.
deadline=$((SECONDS + TIMEOUT))
while true; do
    wait_grounded "$LOG_DIR/saved.status"
    if awk -v a="$(pose "$LOG_DIR/before.status")" -v b="$(pose "$LOG_DIR/saved.status")" \
        'BEGIN { split(a,x); split(b,y); exit !(length(x)==3 && length(y)==3 && (x[1]-y[1])^2+(x[3]-y[3])^2 >= 100) }'; then
        break
    fi
    (( SECONDS < deadline )) || fail 'character did not move at least 10 units before saving'
    sleep 0.25
done
debug_command 'save 1' "$LOG_DIR/save.log" || fail 'save command failed'
grep -Fq 'saved slot 1' "$LOG_DIR/save.log" || fail 'save validation/write failed'
[[ -s "$LOG_DIR/saves/save_1.ess" ]] || fail 'save file missing'
stop_engine
launch restored --load 1
wait_log "$LOG_DIR/restored.stderr" 'save load: restored player pose'
if [[ "${P1_EXPECT_LOADING_SCREEN:-0}" == 1 ]]; then
    wait_log "$LOG_DIR/restored.stderr" 'loading.screen: dismissed after destination frame'
    awk '
        /loading.screen: begin.*owner=Save/ { if (state != 0) exit 1; state=1 }
        /loading.screen: presented before scene teardown/ { if (state != 1) exit 1; state=2 }
        /save load: restored player pose/ { if (state != 2) exit 1; state=3 }
        /loading.screen: dismissed after destination frame/ { if (state != 3) exit 1; state=4 }
        END { if (state != 4) exit 1 }
    ' "$LOG_DIR/restored.stderr" || fail 'save loading cover lifecycle out of order'
fi
wait_grounded "$LOG_DIR/restored.status"
awk -v a="$(pose "$LOG_DIR/saved.status")" -v b="$(pose "$LOG_DIR/restored.status")" \
    'BEGIN { split(a,x); split(b,y); if(length(x)!=3 || length(y)!=3) exit 1; for(i=1;i<=3;i++) if((x[i]-y[i])^2>1) exit 1 }' \
    || fail 'restored body differs from saved pose by more than one unit'
debug_command 'save 2' "$LOG_DIR/resave.log" || fail 'post-restore save command failed'
grep -Fq 'saved slot 2' "$LOG_DIR/resave.log" || fail 'restored world fails save validation'
echo "smoke[p5-save-restart]: PASS -- $FIXTURE_LABEL moved pose survives process restart, grounded and resavable"
echo "artifacts: $LOG_DIR"
