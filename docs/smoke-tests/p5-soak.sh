#!/usr/bin/env bash
# P5 hardening gate 4 — the 30-minute soak with repeated transitions and
# saves. Failures: panic/crash, stuck transition, unbounded memory growth,
# or lost player control.
#
# Cycle design (FNV reference fixture, ~20-40 s per cycle):
#   1. F5 quicksave at the interior standoff (waits for the ring slot).
#   2. Control check: a bound-input walk-out of ≥10 BU — proves the body
#      still answers input every cycle (another direction if an NPC blocks).
#   3. F9 quickload back to the standoff (in-cell session replacement).
#   4. One bound-input E edge opens the authored door; the exterior
#      arrival must land within the transition timeout (stuck-transition
#      detector).
#   5. F9 quickload performs the full exterior→interior session
#      replacement; the restored pose must match the engine's logged
#      restore pose (≤2 units) and the F5-time poll (≤5 units).
# The quickloads, not walks back, keep every cycle at the same standoff: a
# frame-counted walk back drifted with the frame rate and NPC traffic.
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
    # A boot-time force-greet holds the dialogue page and clears injected
    # input every frame; end it before the first control walk (#6052).
    smoke_end_boot_conversation "$LOG_DIR/dialogue-end.log"
    smoke_answer_message_boxes "$LOG_DIR/answer.log"
}

# F9 the newest quicksave and require the body back at this cycle's
# standoff. Restore-fidelity reference: the engine's OWN restore line is the
# pose it saved at F5-drain time; compare the settled body against it (≤2
# units) and loosely against the pre-F5 poll (≤5 units, still catches a
# wrong-slot restore — a poll can be seconds stale on the sloped porch,
# observed 2026-10-01).
quickload_to_standoff() { # <cycle> <label>
    local cycle="$1" label="$2" rc=0
    debug_command 'input.press quickload' "$LOG_DIR/c$cycle.$label.f9.log" \
        || fail "cycle $cycle: quickload press rejected ($label)"
    grep -Fq 'action=Quickload binding=F9' "$LOG_DIR/c$cycle.$label.f9.log" \
        || fail "cycle $cycle: F9 edge not recognized ($label)"
    local restore_deadline=$((SECONDS + TIMEOUT))
    until tail -c +"$((stderr_mark + 1))" "$LOG_DIR/session.stderr" \
          | grep -Fq 'save load: restored player pose'; do
        kill -0 "$engine_pid" 2>/dev/null \
            || fail "cycle $cycle: engine died during quickload ($label; see artifacts)"
        (( SECONDS < restore_deadline )) || fail "cycle $cycle: quickload restore stuck >${TIMEOUT}s ($label)"
        sleep 0.5
    done
    stderr_mark=$(stat -c %s "$LOG_DIR/session.stderr")
    wait_grounded "$LOG_DIR/c$cycle.$label.restored.status" || rc=$?
    if (( rc != 0 )); then
        debug_command phys.stats "$LOG_DIR/c$cycle.$label.failed.phys.stats" 2>/dev/null || true
        debug_command phys.census "$LOG_DIR/c$cycle.$label.failed.phys.census" 2>/dev/null || true
        if (( rc == 2 )); then
            fail "cycle $cycle: player status unresponsive after quickload ($label; lost control)"
        else
            fail "cycle $cycle: body never re-grounded after quickload ($label; see samples + phys census)"
        fi
    fi
    grep 'save load: restored player pose' "$LOG_DIR/session.stderr" | tail -1 \
        | sed -nE 's/.*restored player pose at \(([-0-9.]+), ([-0-9.]+), ([-0-9.]+)\).*/\1 \2 \3/p' \
        >"$LOG_DIR/c$cycle.$label.restored.pose"
    [[ -s "$LOG_DIR/c$cycle.$label.restored.pose" ]] \
        || fail "cycle $cycle: no restored-pose line for this quickload ($label)"
    awk -v a="$(pose "$LOG_DIR/c$cycle.$label.restored.status")" \
        -v b="$(cat "$LOG_DIR/c$cycle.$label.restored.pose")" \
        'BEGIN { split(a,x); split(b,y); if(length(x)!=3 || length(y)!=3) exit 1; for(i=1;i<=3;i++) if((x[i]-y[i])^2>4) exit 1 }' \
        || fail "cycle $cycle: restored body ≠ engine-logged restore pose (>2 units — #5155 class; $label)"
    pose_close "$LOG_DIR/c$cycle.$label.restored.status" "$LOG_DIR/c$cycle.pre.status" 25 \
        || fail "cycle $cycle: restored pose is far from the F5-time pose (>5 units — wrong-slot restore? $label)"
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

    # 1 — F5 quicksave at the interior standoff, before anything moves the
    # body. The quicksave is deferred: wait for its ring slot to land so the
    # walk below cannot reach the saved pose.
    if ! wait_grounded "$LOG_DIR/c$cycle.pre.status"; then
        fail "cycle $cycle: player status unresponsive before the quicksave (lost control)"
    fi
    touch "$LOG_DIR/c$cycle.f5.mark"
    debug_command 'input.press quicksave' "$LOG_DIR/c$cycle.f5.log" \
        || fail "cycle $cycle: quicksave press rejected"
    grep -Fq 'action=Quicksave binding=F5' "$LOG_DIR/c$cycle.f5.log" \
        || fail "cycle $cycle: F5 edge not recognized"
    save_deadline=$((SECONDS + TIMEOUT))
    until [[ -n "$(find "$LOG_DIR/saves" -name '*.ess' -newer "$LOG_DIR/c$cycle.f5.mark" 2>/dev/null)" ]]; do
        (( SECONDS < save_deadline )) || fail "cycle $cycle: the F5 quicksave never wrote its slot"
        sleep 0.25
    done
    quicksaves=$((quicksaves + 1))

    # 2 — control: a bound-input walk-out of ≥10 units. An NPC crossing the
    # aisle by the door (the saloon's own traffic) can block one direction
    # for a moment; control is lost only when no direction moves the body.
    walked_out=0
    for direction in backward left right forward; do
        debug_command "input.hold $direction 90" "$LOG_DIR/c$cycle.walk.log" \
            || fail "cycle $cycle: walk-out input rejected"
        wait_grounded "$LOG_DIR/c$cycle.walked.status" || fail "cycle $cycle: lost control during walk-out"
        if pose_apart "$LOG_DIR/c$cycle.walked.status" "$LOG_DIR/c$cycle.pre.status" 100; then
            walked_out=1
            break
        fi
        echo "smoke[p5-soak]: cycle $cycle: walk-out $direction blocked; retrying another way"
    done
    (( walked_out )) || fail "cycle $cycle: walk-out did not displace the body ≥10 units in any direction (lost control)"

    # 3 — F9 back to the standoff: an in-cell session replacement. The
    # quickload, not a walk back, returns the body — a frame-counted walk
    # back drifts with the frame rate (headless ~31 FPS vs the desktop's
    # ~140) and with whatever crossed the aisle, out of the door's reach.
    quickload_to_standoff "$cycle" walked

    # 4 — the door out: prompt → E edge → authored exterior arrival.
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

    # 5 — F9 back inside from the exterior: the full cross-cell session
    # replacement, pose back at the standoff.
    quickload_to_standoff "$cycle" exterior

    # 6 — memory sample.
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
