#!/usr/bin/env bash
# P5 hardening gate — the round trip through a door: out of the fixture's
# interior, then back in through the door on the far side, driven by the
# console's `door.teleport` so no walking or frame-counted input decides whether
# the second door is reached.
#
# The second door is the one that matters (#6608). An exterior drags ground
# cover, terrain and a large geometry SSBO into the transition back, none of
# which the first door has; on Skyrim the second door lost the Vulkan device
# (Xid 109) while a loading-stage frame re-dispatched the last exterior frame's
# ground-cover scatter over a compacted vertex SSBO.
#
# Gate:
#   door.teleport (interior -> exterior) -> ground cover live, when the fixture
#   says the route has it -> door.teleport (exterior -> interior) -> both
#   transitions applied, the engine alive and no device loss afterwards.
set -euo pipefail
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/fixture.sh"
smoke_load_fixture p5-door-round-trip "$@"
smoke_require_fixture_fields P0_CELL
smoke_require_data
# The fixture states the route was measured. FNV's does not: its Prospector
# arrival stands past INTERACTION_REACH_BU from the door, so the second door
# cannot be targeted from the arrival pose.
smoke_require_fixture_fields P5RT_ROUTE
cd "$SMOKE_ROOT_DIR"
PORT="${BYRO_DEBUG_PORT:-19880}"
TIMEOUT="${BYROREDUX_SMOKE_TIMEOUT:-300}"
LOG_DIR="$(mktemp -d /tmp/byro-p5-round-trip.XXXXXX)"
# A failure names $LOG_DIR as its artifacts, so it keeps them;
# BYROREDUX_KEEP_ARTIFACTS=1 keeps a passing run's too.
keep_artifacts="${BYROREDUX_KEEP_ARTIFACTS:-0}"
engine_pid=""
# The engine runs under xvfb-run in its own session: a TERM to the wrapper alone
# orphans the engine, which then holds the port and the GPU.
stop_engine() {
    if [[ -n "$engine_pid" ]]; then
        kill -TERM -- "-$engine_pid" 2>/dev/null || true
        wait "$engine_pid" 2>/dev/null || true
        engine_pid=""
    fi
}
cleanup() {
    stop_engine
    if (( keep_artifacts == 0 )); then
        rm -rf "$LOG_DIR"
    fi
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
fail() {
    keep_artifacts=1
    echo "smoke[p5-door-round-trip]: FAIL -- $*" >&2
    echo "artifacts: $LOG_DIR" >&2
    exit 1
}
if ss -ltn "sport = :$PORT" | grep -q LISTEN; then
    fail "port $PORT already occupied; refusing to attach to another session"
fi
cargo build --release --offline --quiet -p byroredux -p byro-dbg
debug_command() {
    timeout "$TIMEOUT" env BYRO_DEBUG_PORT="$PORT" target/release/byro-dbg >"$2" 2>&1 <<COMMANDS
$1
.quit
COMMANDS
}
engine_log="$LOG_DIR/engine.stderr"
wait_log() {
    local pattern="$1" deadline=$((SECONDS + TIMEOUT))
    until grep -Eq "$pattern" "$engine_log"; do
        kill -0 "$engine_pid" 2>/dev/null || fail "engine exited waiting for $pattern"
        (( SECONDS < deadline )) || fail "timeout waiting for $pattern"
        sleep 0.25
    done
}
applied_count() { grep -c 'Cell transition applied' "$engine_log" || true; }
wait_applied() {
    local want="$1" deadline=$((SECONDS + TIMEOUT))
    until (( $(applied_count) >= want )); do
        grep -q 'logical device has been lost' "$engine_log" && fail "device lost before transition $want applied"
        kill -0 "$engine_pid" 2>/dev/null || fail "engine exited before transition $want applied"
        (( SECONDS < deadline )) || fail "timeout waiting for transition $want"
        sleep 0.25
    done
}
# Sets `door` to the door in reach, as `interaction.status` reports it. An arrival
# can face away from the door it came through, so the player turns until the ray
# finds it.
door_target() {
    local output="$LOG_DIR/status.$1.log" deadline=$((SECONDS + 60)) yaw=0
    while (( SECONDS < deadline )); do
        debug_command interaction.status "$output" || fail 'interaction status unavailable'
        door="$(sed -nE 's/.*target=([0-9]+) kind=Door.*/\1/p' "$output" | tail -1)"
        [[ -z "$door" ]] || return 0
        debug_command "input.look $yaw 0" "$LOG_DIR/look.$1.log" || fail 'input.look unavailable'
        yaw=$(( (yaw + 30) % 360 ))
        sleep 1
    done
    fail "no door in reach for $1 (see $output)"
}
teleport_through() {
    local door=""
    door_target "$1"
    echo "smoke[p5-door-round-trip]: $1 door $door"
    debug_command "door.teleport $door" "$LOG_DIR/teleport.$1.log" || fail "door.teleport $door failed"
    grep -Fq 'teleport payload' "$LOG_DIR/teleport.$1.log" || fail "door $door has no teleport payload"
}

echo "================================================================"
echo "  smoke[p5-door-round-trip]: $FIXTURE_LABEL -- $P0_CELL and back"
echo "================================================================"

setsid env BYRO_DEBUG_PORT="$PORT" BYRO_DEBUG_SERVER=1 \
    RUST_LOG="warn,engine::stats=info,byroredux::loading_screen=info,byroredux::cell_loader::transition=info,byroredux::app_step=info" \
    "${SMOKE_HEADLESS[@]}" target/release/byroredux "${SMOKE_ENGINE_ARGS[@]}" \
    --cell "$P0_CELL" --player \
    --bench-frames 30 --bench-hold \
    >"$LOG_DIR/engine.stdout" 2>"$engine_log" &
engine_pid=$!
wait_log 'bench-hold:'
smoke_end_boot_conversation "$LOG_DIR/dialogue-end.log" boot

teleport_through out
wait_applied 1
# Long enough for the exterior's loading cover to lift and its streaming to
# settle, so the second door tears down a live exterior, not a half-built one.
sleep "${BYROREDUX_SMOKE_SETTLE:-20}"

if [[ -n "${P5RT_EXPECT_GROUNDCOVER:-}" ]]; then
    # Without live ground cover this route proves nothing about #6608.
    grep -Eq 'groundcover_scatter=[0-9]' "$engine_log" \
        || fail "the exterior never dispatched ground cover, so the route has lost its hazard"
fi

teleport_through back
wait_applied 2
sleep 10
kill -0 "$engine_pid" 2>/dev/null || fail "engine exited after the second transition"
if grep -Eq 'logical device has been lost|Draw failed' "$engine_log"; then
    fail "device lost after the second transition"
fi

echo "smoke[p5-door-round-trip]: PASS -- $FIXTURE_LABEL round trip through $P0_CELL's door"
