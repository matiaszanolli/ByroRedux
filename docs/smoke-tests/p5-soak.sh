#!/usr/bin/env bash
# P5 hardening gate 4 — the 30-minute soak with repeated transitions and
# saves. Failures: panic/crash, stuck transition, unbounded memory growth,
# or lost player control.
#
# Cycle design (FNV reference fixture, ~1-2 min per cycle):
#   1. Control check: bound-input walk out and back (≥10 BU each way) —
#      proves the body still answers input every cycle.
#   2. F5 quicksave at the interior standoff.
#   3. One bound-input E edge opens the authored door; the exterior
#      arrival must land within the transition timeout (stuck-transition
#      detector).
#   4. F9 quickload performs the full exterior→interior session
#      replacement; the restored pose must match the standoff (≤1 unit).
# Every cycle samples VmRSS; after a 3-cycle warmup, growth beyond 1.5× the
# warmup-end RSS fails as unbounded memory growth. The full curve is
# retained in the artifacts.
#
# Duration is BYROREDUX_SOAK_MINUTES (default 30). Shorten it to shake the
# script itself out; the P5 gate of record runs the default.
set -euo pipefail
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/fixture.sh"
smoke_load_fixture p5-soak "$@"
smoke_require_fixture_fields \
    P0_CELL P0_CAMERA_POS P0_CAMERA_FORWARD P0_PROMPT P0_APPLIED_LOG P1_ARRIVAL_GRID
smoke_require_data
cd "$SMOKE_ROOT_DIR"
PORT="${BYRO_DEBUG_PORT:-19873}"
TIMEOUT="${BYROREDUX_SMOKE_TIMEOUT:-300}"
SOAK_MINUTES="${BYROREDUX_SOAK_MINUTES:-30}"
LOG_DIR="$(mktemp -d /tmp/byro-p5-soak.XXXXXX)"
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
    echo "smoke[p5-soak]: FAIL -- $*" >&2
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
rss_kb() {
    awk '/^VmRSS:/ { print $2 }' "/proc/$engine_pid/status" 2>/dev/null || echo 0
}
pose() {
    sed -nE 's/.*body=\(([-0-9.]+), ([-0-9.]+), ([-0-9.]+)\).*/\1 \2 \3/p' "$1" | tail -1
}
pose_close() { # <fileA> <fileB> <max squared distance>
    awk -v a="$(pose "$1")" -v b="$(pose "$2")" -v max="$3" \
        'BEGIN { split(a,x); split(b,y); if(length(x)!=3 || length(y)!=3) exit 1; for(i=1;i<=3;i++) if((x[i]-y[i])^2>max) exit 1 }'
}
pose_apart() { # <fileA> <fileB> <min squared distance>
    awk -v a="$(pose "$1")" -v b="$(pose "$2")" -v min="$3" \
        'BEGIN { split(a,x); split(b,y); exit !(length(x)==3 && length(y)==3 && (x[1]-y[1])^2+(x[2]-y[2])^2+(x[3]-y[3])^2 >= min) }'
}
wait_grounded() {
    local output="$1" deadline=$((SECONDS + TIMEOUT))
    local sample=0 second=0
    while true; do
        # Once a second, retain a numbered sample of the body state — the
        # trajectory (pose/grounded/velocity) is what distinguishes "stuck at
        # the restored pose with grounded=false" from "falling" when this
        # wait fails. The caller's named file always holds the latest poll.
        second=$((second + 1))
        if (( second % 4 == 1 )); then
            sample=$((sample + 1))
            debug_command player.status "$output.$sample" || return 2
            cp "$output.$sample" "$output"
        else
            debug_command player.status "$output" || return 2
        fi
        if grep -Fq 'grounded=true' "$output" &&
           grep -Fq 'input_hold_frames_remaining=0' "$output"; then
            return 0
        fi
        (( SECONDS < deadline )) || return 1
        sleep 0.25
    done
}

launch() {
    env BYRO_DEBUG_PORT="$PORT" BYRO_DEBUG_SERVER=1 \
        BYROREDUX_SAVE_DIR="$LOG_DIR/saves" \
        BYROREDUX_SETTINGS_PATH="$LOG_DIR/settings.toml" \
        RUST_LOG="warn,byroredux::save_io=info,byroredux::loading_screen=info,byroredux::interaction=info,byroredux::cell_loader::transition=info,byroredux::app_step=info" \
        target/release/byroredux "${SMOKE_ENGINE_ARGS[@]}" \
        --cell "$P0_CELL" --player --radius 1 \
        --camera-pos "$P0_CAMERA_POS" --camera-forward "$P0_CAMERA_FORWARD" \
        --bench-frames 30 --bench-hold \
        >"$LOG_DIR/session.stdout" 2>"$LOG_DIR/session.stderr" &
    engine_pid=$!
    wait_log "$LOG_DIR/session.stderr" 'bench-hold:'
}

echo "═══════════════════════════════════════════════════════════════"
echo "  smoke[p5-soak]: $SOAK_MINUTES-minute transition/save soak ($FIXTURE_LABEL)"
echo "═══════════════════════════════════════════════════════════════"
launch
wait_grounded "$LOG_DIR/pose0.status" || fail 'player did not settle grounded at startup'
echo "cycle,time_s,rss_kb,quicksaves" >"$LOG_DIR/soak.csv"

deadline_s=$(( SOAK_MINUTES * 60 ))
SECONDS=0
cycle=0
warmup_rss=0
max_rss=0
quicksaves=0
stderr_mark=0
applied_count=0
while (( SECONDS < deadline_s )); do
    cycle=$((cycle + 1))

    # 1 — control: walk away and back, ≥10 BU each way. Displacement is
    # measured against THIS cycle's starting pose, not the original spawn —
    # after the first F9 the body lives at the previous cycle's quicksave
    # point, which is near but not exactly the standoff.
    if ! wait_grounded "$LOG_DIR/c$cycle.pre.status"; then
        fail "cycle $cycle: player status unresponsive before the walk (lost control)"
    fi
    debug_command "input.hold backward 90" "$LOG_DIR/c$cycle.walk1.log" \
        || fail "cycle $cycle: walk-out input rejected"
    walk_deadline=$((SECONDS + TIMEOUT))
    while true; do
        wait_grounded "$LOG_DIR/c$cycle.walked.status" || fail "cycle $cycle: lost control during walk-out"
        pose_apart "$LOG_DIR/c$cycle.walked.status" "$LOG_DIR/c$cycle.pre.status" 100 && break
        (( SECONDS < walk_deadline )) || fail "cycle $cycle: walk-out did not displace the body ≥10 units (lost control)"
        sleep 0.25
    done
    debug_command "input.hold forward 90" "$LOG_DIR/c$cycle.walk2.log" \
        || fail "cycle $cycle: walk-back input rejected"
    walk_deadline=$((SECONDS + TIMEOUT))
    while true; do
        wait_grounded "$LOG_DIR/c$cycle.back.status" || fail "cycle $cycle: lost control during walk-back"
        pose_apart "$LOG_DIR/c$cycle.back.status" "$LOG_DIR/c$cycle.walked.status" 100 && break
        (( SECONDS < walk_deadline )) || fail "cycle $cycle: walk-back did not return ≥10 units (lost control)"
        sleep 0.25
    done

    # 2 — F5 quicksave at the interior standoff (before the door).
    debug_command 'input.press quicksave' "$LOG_DIR/c$cycle.f5.log" \
        || fail "cycle $cycle: quicksave press rejected"
    grep -Fq 'action=Quicksave binding=F5' "$LOG_DIR/c$cycle.f5.log" \
        || fail "cycle $cycle: F5 edge not recognized"
    quicksaves=$((quicksaves + 1))

    # 3 — the door out: prompt → E edge → authored exterior arrival.
    debug_command 'interaction.status' "$LOG_DIR/c$cycle.prompt.log" \
        || fail "cycle $cycle: interaction status unavailable"
    grep -Fq "$P0_PROMPT" "$LOG_DIR/c$cycle.prompt.log" \
        || fail "cycle $cycle: door prompt missing (world state drifted?)"
    debug_command 'input.press activate' "$LOG_DIR/c$cycle.press.log" \
        || fail "cycle $cycle: activate press rejected"
    grep -Fq 'input.press: queued action=Activate binding=E' "$LOG_DIR/c$cycle.press.log" \
        || fail "cycle $cycle: E binding edge not queued"
    applied_count=$((applied_count + 1))
    arrival_deadline=$((SECONDS + TIMEOUT))
    until tail -c +"$((stderr_mark + 1))" "$LOG_DIR/session.stderr" \
          | grep -Fq "$P0_APPLIED_LOG"; do
        kill -0 "$engine_pid" 2>/dev/null \
            || fail "cycle $cycle: engine died during the door transition (see artifacts)"
        (( SECONDS < arrival_deadline )) || fail "cycle $cycle: transition stuck >${TIMEOUT}s"
        sleep 0.5
    done
    stderr_mark=$(stat -c %s "$LOG_DIR/session.stderr")
    until debug_command player.status "$LOG_DIR/c$cycle.arrival.status" 2>/dev/null &&
          grep -Fq "$P1_ARRIVAL_GRID" "$LOG_DIR/c$cycle.arrival.status"; do
        kill -0 "$engine_pid" 2>/dev/null \
            || fail "cycle $cycle: engine died before the exterior arrival"
        (( SECONDS < arrival_deadline )) || fail "cycle $cycle: exterior arrival never landed (transition stuck)"
        sleep 1
    done

    # 4 — F9 back inside: full session replacement, pose back at the standoff.
    debug_command 'input.press quickload' "$LOG_DIR/c$cycle.f9.log" \
        || fail "cycle $cycle: quickload press rejected"
    grep -Fq 'action=Quickload binding=F9' "$LOG_DIR/c$cycle.f9.log" \
        || fail "cycle $cycle: F9 edge not recognized"
    restore_deadline=$((SECONDS + TIMEOUT))
    until tail -c +"$((stderr_mark + 1))" "$LOG_DIR/session.stderr" \
          | grep -Fq 'save load: restored player pose'; do
        kill -0 "$engine_pid" 2>/dev/null \
            || fail "cycle $cycle: engine died during quickload (see artifacts)"
        (( SECONDS < restore_deadline )) || fail "cycle $cycle: quickload restore stuck >${TIMEOUT}s"
        sleep 0.5
    done
    stderr_mark=$(stat -c %s "$LOG_DIR/session.stderr")
    rc=0
    wait_grounded "$LOG_DIR/c$cycle.restored.status" || rc=$?
    if (( rc != 0 )); then
        debug_command phys.stats "$LOG_DIR/c$cycle.failed.phys.stats" 2>/dev/null || true
        debug_command phys.census "$LOG_DIR/c$cycle.failed.phys.census" 2>/dev/null || true
        if (( rc == 2 )); then
            fail "cycle $cycle: player status unresponsive after quickload (lost control)"
        else
            fail "cycle $cycle: body never re-grounded after quickload (see c$cycle.restored.status.* samples + phys census)"
        fi
    fi
    # The restored pose must match the pose at F5 time (this cycle's
    # walk-back endpoint) — the standoff reference itself moves with the
    # quicksave point, so the comparison is cycle-local.
    pose_close "$LOG_DIR/c$cycle.restored.status" "$LOG_DIR/c$cycle.back.status" 1 \
        || fail "cycle $cycle: restored pose drifted from the quicksaved pose (>1 unit)"

    # 5 — memory sample.
    sample_rss="$(rss_kb)"
    (( sample_rss > max_rss )) && max_rss=$sample_rss
    if (( cycle == 3 )); then
        warmup_rss=$sample_rss
        echo "smoke[p5-soak]: warmup complete — RSS baseline ${warmup_rss} kB at cycle 3"
    fi
    echo "$cycle,$SECONDS,$sample_rss,$quicksaves" >>"$LOG_DIR/soak.csv"
    echo "smoke[p5-soak]: cycle $cycle @ ${SECONDS}s — rss=$(( sample_rss / 1024 ))MiB quicksaves=$quicksaves transitions=$applied_count"

    if (( warmup_rss > 0 )) && (( sample_rss > warmup_rss * 3 / 2 )); then
        fail "unbounded memory growth: cycle $cycle RSS ${sample_rss} kB > 1.5× warmup ${warmup_rss} kB (curve in soak.csv)"
    fi

    if grep -Fq 'panicked at' "$LOG_DIR/session.stderr"; then
        fail "engine panicked during cycle $cycle (see session.stderr)"
    fi
done

kill -0 "$engine_pid" 2>/dev/null || fail "engine died before the soak deadline"
echo "smoke[p5-soak]: soak curve (cycle,time_s,rss_kb,quicksaves):"
cat "$LOG_DIR/soak.csv"
echo "smoke[p5-soak]: PASS -- ${SOAK_MINUTES} min, $cycle cycles, $quicksaves quicksaves, $applied_count door transitions, peak RSS $(( max_rss / 1024 ))MiB, no panic / stuck transition / control loss / unbounded growth"
echo "artifacts: $LOG_DIR"
