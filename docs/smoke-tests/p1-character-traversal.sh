#!/usr/bin/env bash
# Playable-slice P1 traversal gate: drive a real CharacterController through
# an interior, both sides of its XTEL door, and an exterior streaming
# boundary. Debug commands are deterministic input frontends only: movement
# still flows through ActionBindings -> ActionState -> Rapier KCC, and
# input.look writes the same yaw/pitch accumulator as mouse look.
#
# Game-parameterised (#3039): cell, camera pose, destination tokens and the
# whole exterior route come from `fixtures/<game>.env`. Pass the game as the
# first argument (or set `BYROREDUX_SMOKE_GAME`); default `skyrim_se`.
#
#   docs/smoke-tests/p1-character-traversal.sh          # Skyrim SE
#   docs/smoke-tests/p1-character-traversal.sh fnv      # Fallout New Vegas

set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/fixture.sh"
smoke_load_fixture p1-character-traversal "$@"
smoke_require_fixture_fields \
    P1_CELL P1_CAMERA_POS P1_CAMERA_FORWARD P1_PROMPT P1_WALK_AWAY_FRAMES \
    P1_WALK_BACK_FRAMES P1_OUTBOUND_LOG P1_ARRIVAL_GRID P1_RETURN_LOG \
    P1_RETURN_OUTCOME P1_ENTITY_FLOOR P1_MIN_CROSSINGS

ROOT_DIR="$SMOKE_ROOT_DIR"
ENGINE_BIN="$ROOT_DIR/target/release/byroredux"
DEBUG_BIN="$ROOT_DIR/target/release/byro-dbg"
PORT="${BYRO_DEBUG_PORT:-9876}"
BENCH_FRAMES="${BYROREDUX_SMOKE_FRAMES:-30}"
TIMEOUT="${BYROREDUX_SMOKE_TIMEOUT:-360}"

LOG_DIR="$(mktemp -d /tmp/byro-p1-traversal.XXXXXX)"
engine_pid=""
keep_artifacts=0
cleanup() {
    if [[ -n "$engine_pid" ]]; then
        kill -TERM "$engine_pid" 2>/dev/null || true
        wait "$engine_pid" 2>/dev/null || true
    fi
    if (( keep_artifacts == 0 )); then
        rm -rf "$LOG_DIR"
    fi
}
trap cleanup EXIT INT TERM

fail() {
    keep_artifacts=1
    echo "smoke[p1-character-traversal]: FAIL -- $*"
    echo "smoke[p1-character-traversal]: artifacts retained at $LOG_DIR"
    tail -60 "$LOG_DIR/engine.stderr" 2>/dev/null || true
    exit 1
}

smoke_require_data

if [[ ! -x "$ENGINE_BIN" || ! -x "$DEBUG_BIN" ]]; then
    echo "smoke[p1-character-traversal]: building release binaries"
    (cd "$ROOT_DIR" && cargo build --release --quiet -p byroredux -p byro-dbg)
fi

engine_stdout="$LOG_DIR/engine.stdout"
engine_stderr="$LOG_DIR/engine.stderr"
status_log="$LOG_DIR/player.status"
command_log="$LOG_DIR/command.log"
interaction_log="$LOG_DIR/interaction.status"

debug_command() {
    local command="$1"
    local output="$2"
    env BYRO_DEBUG_PORT="$PORT" "$DEBUG_BIN" >"$output" 2>&1 <<EOF
$command
.quit
EOF
}

wait_for_debug_pattern() {
    local command="$1"
    local pattern="$2"
    local output="$3"
    local description="$4"
    local deadline=$(( $(date +%s) + TIMEOUT ))
    while true; do
        if debug_command "$command" "$output" && grep -Fq "$pattern" "$output"; then
            echo "smoke[p1-character-traversal]: PASS -- $description"
            return 0
        fi
        if (( $(date +%s) > deadline )); then
            cat "$output" 2>/dev/null || true
            fail "timeout waiting for $description ('$pattern')"
        fi
        kill -0 "$engine_pid" 2>/dev/null || fail "engine exited while waiting for $description"
        sleep 0.25
    done
}

run_hold() {
    local action="$1"
    local frames="$2"
    local description="$3"
    # Re-seed the gameplay look accumulator for every segment. A windowed
    # smoke still receives compositor mouse-motion events; allowing those to
    # accumulate across dozens of chunks turns an intended +X strafe into an
    # arbitrary world direction and can steer the fixture into another door.
    debug_command "input.look 0 0" "$command_log" \
        || fail "could not stabilize look for $description"
    grep -Fq "yaw=0.0° pitch=0.0°" "$command_log" \
        || fail "$description look stabilization was not applied"
    debug_command "input.hold $action $frames" "$command_log" \
        || fail "could not queue $description"
    grep -Fq "for $frames frames" "$command_log" \
        || fail "$description did not enter through input.hold"

    # The debug server acknowledges the queue command before the next Early
    # stage necessarily consumes it. Observe a non-zero countdown first so a
    # stale pre-hold status cannot make this helper declare completion and let
    # the following waypoint overwrite an input that has not started yet.
    local start_deadline=$(( $(date +%s) + 10 ))
    local remaining=""
    while true; do
        debug_command "player.status" "$status_log" || true
        remaining="$(grep -oE 'input_hold_frames_remaining=[0-9]+' "$status_log" \
            | tail -1 | cut -d= -f2 || true)"
        if [[ "$remaining" =~ ^[0-9]+$ ]] && (( remaining > 0 )); then
            break
        fi
        if (( $(date +%s) > start_deadline )); then
            cat "$status_log" 2>/dev/null || true
            fail "$description was acknowledged but its countdown never started"
        fi
        kill -0 "$engine_pid" 2>/dev/null || fail "engine exited before $description started"
        sleep 0.05
    done
    wait_for_debug_pattern \
        "player.status" \
        "input_hold_frames_remaining=0" \
        "$status_log" \
        "$description completed"
    wait_for_debug_pattern \
        "player.status" \
        "grounded=true" \
        "$status_log" \
        "$description grounded settle"
}

body_coordinate() {
    local axis="$1"
    debug_command "player.status" "$status_log" || return 1
    case "$axis" in
        x)
            sed -nE 's/.*body=\(([-0-9.]+),.*/\1/p' "$status_log" | tail -1
            ;;
        z)
            sed -nE 's/.*body=\([-0-9.]+, [-0-9.]+, ([-0-9.]+)\).*/\1/p' \
                "$status_log" | tail -1
            ;;
        *)
            return 1
            ;;
    esac
}

coordinate_reached() {
    local value="$1"
    local comparison="$2"
    local target="$3"
    case "$comparison" in
        le) awk -v value="$value" -v target="$target" 'BEGIN { exit !(value <= target) }' ;;
        ge) awk -v value="$value" -v target="$target" 'BEGIN { exit !(value >= target) }' ;;
        *) return 1 ;;
    esac
}

# Reach a world-space waypoint through repeated bounded gameplay holds. Logical
# frame time intentionally follows the live engine clock, so position — not a
# fixed frame count — is the deterministic completion condition.
move_until() {
    local action="$1"
    local axis="$2"
    local comparison="$3"
    local target="$4"
    local description="$5"
    local chunk_frames="${6:-100}"
    local attempt current
    for attempt in $(seq 1 20); do
        current="$(body_coordinate "$axis")"
        [[ "$current" =~ ^-?[0-9]+([.][0-9]+)?$ ]] \
            || fail "$description could not parse player $axis coordinate"
        if coordinate_reached "$current" "$comparison" "$target"; then
            # A KCC support edge can report one transient airborne sample even
            # after the hold completed. Require it to settle at the waypoint
            # rather than treating that single sample as a traversal failure.
            wait_for_debug_pattern \
                "player.status" \
                "grounded=true" \
                "$status_log" \
                "$description grounded settle"
            echo "smoke[p1-character-traversal]: PASS -- $description ($axis=$current)"
            return 0
        fi
        run_hold "$action" "$chunk_frames" "$description segment $attempt"
    done
    fail "$description did not reach $axis $comparison $target (last $axis=$current)"
}

# Drive one fixture-declared route leg. See the P1_ROUTE comment in
# `fixtures/<game>.env` for the two leg forms.
run_route_leg() {
    local leg="$1"
    local IFS='|'
    # shellcheck disable=SC2206
    local field=($leg)
    unset IFS
    case "${field[0]}" in
        hold)
            run_hold "${field[1]}" "${field[2]}" "${field[3]}"
            if [[ -n "${field[4]:-}" ]]; then
                grep -Fq "${field[4]}" "$status_log" \
                    || fail "${field[3]} did not leave the player at ${field[4]}"
                echo "smoke[p1-character-traversal]: PASS -- ${field[3]} reached ${field[4]}"
            fi
            ;;
        until)
            move_until "${field[1]}" "${field[2]}" "${field[3]}" "${field[4]}" \
                "${field[6]}" "${field[5]}"
            if [[ -n "${field[7]:-}" ]]; then
                grep -Fq "${field[7]}" "$status_log" \
                    || fail "${field[6]} did not leave the player at ${field[7]}"
                echo "smoke[p1-character-traversal]: PASS -- ${field[6]} reached ${field[7]}"
            fi
            ;;
        *)
            fail "unknown P1_ROUTE leg form '${field[0]}' in $SMOKE_GAME.env"
            ;;
    esac
}

wait_for_engine_log() {
    local pattern="$1"
    local description="$2"
    local deadline=$(( $(date +%s) + TIMEOUT ))
    while ! grep -Fq "$pattern" "$engine_stderr" 2>/dev/null; do
        if (( $(date +%s) > deadline )); then
            fail "timeout waiting for $description ('$pattern')"
        fi
        kill -0 "$engine_pid" 2>/dev/null || fail "engine exited while waiting for $description"
        sleep 0.25
    done
    echo "smoke[p1-character-traversal]: PASS -- $description"
}

echo "================================================================"
echo "  smoke[p1-character-traversal]: $FIXTURE_LABEL -- $P1_HEADLINE"
echo "================================================================"

cd "$ROOT_DIR"
env BYRO_DEBUG_PORT="$PORT" \
    RUST_LOG="error,byroredux::interaction=info,byroredux::cell_loader::transition=info,byroredux::app_step=info" \
    "$ENGINE_BIN" \
    "${SMOKE_ENGINE_ARGS[@]}" \
    --cell "$P1_CELL" \
    --player \
    --camera-pos="$P1_CAMERA_POS" \
    --camera-forward="$P1_CAMERA_FORWARD" \
    --radius 1 \
    --bench-frames "$BENCH_FRAMES" \
    --bench-hold \
    >"$engine_stdout" 2>"$engine_stderr" &
engine_pid=$!

wait_for_engine_log "bench-hold:" "engine reached the held interactive state"
wait_for_debug_pattern "player.status" "mode=Character" "$status_log" "Character mode is active"
wait_for_debug_pattern "player.status" "grounded=true" "$status_log" "interior spawn settled on the floor"
wait_for_debug_pattern "interaction.status" "$P1_PROMPT" "$interaction_log" "interior XTEL prompt is reachable"

# Prove free walking and a collision-stable return to the threshold before
# using the door. The extra return frames account for the capsule nudge.
run_hold forward "$P1_WALK_AWAY_FRAMES" "interior walk-away"
debug_command "interaction.status" "$interaction_log"
grep -Fq "target=none prompt=none" "$interaction_log" \
    || fail "interior walk-away did not leave the door's interaction volume"
run_hold backward "$P1_WALK_BACK_FRAMES" "interior threshold return"
wait_for_debug_pattern "interaction.status" "$P1_PROMPT" "$interaction_log" "door prompt recovered after walking"

debug_command "input.press activate" "$command_log" || fail "could not activate the interior door"
grep -Fq "input.press: queued action=Activate binding=E" "$command_log" \
    || fail "interior door activation bypassed the action binding"
wait_for_engine_log "$P1_OUTBOUND_LOG" "interior-to-exterior transition completed"
wait_for_debug_pattern "player.status" "$P1_ARRIVAL_GRID" "$status_log" "exterior arrival grid is correct"
grep -Fq "grounded=true" "$status_log" || fail "exterior arrival was not grounded"

# Freeze yaw through the same InputState accumulator mouse look owns, then
# traverse authored collision. Every leg is a normal KCC step through held
# gameplay input; no capsule teleport participates in the route.
debug_command "input.look 0 0" "$command_log" || fail "could not seed traversal yaw"
grep -Fq "yaw=0.0° pitch=0.0°" "$command_log" || fail "traversal yaw was not applied"
for leg in "${P1_ROUTE[@]}"; do
    run_route_leg "$leg"
done
wait_for_debug_pattern "interaction.status" "$P1_PROMPT" "$interaction_log" "reverse exterior door prompt is reachable"

debug_command "input.press activate" "$command_log" || fail "could not activate the reverse door"
grep -Fq "input.press: queued action=Activate binding=E" "$command_log" \
    || fail "reverse door activation bypassed the action binding"
wait_for_engine_log "$P1_RETURN_LOG" "exterior-to-interior transition completed"
wait_for_debug_pattern "player.status" "mode=Character" "$status_log" "character control survived the round trip"
wait_for_debug_pattern "player.status" "grounded=true" "$status_log" "round-trip interior arrival settled on the floor"
wait_for_debug_pattern "interaction.status" "activations=2" "$interaction_log" "exactly two door activations were consumed"
grep -Fq "$P1_RETURN_OUTCOME" "$interaction_log" \
    || fail "reverse-door outcome was not retained"

crossings="$(grep -Fc "Player crossed cell boundary" "$engine_stderr" || true)"
(( crossings >= P1_MIN_CROSSINGS )) \
    || fail "streaming telemetry reported only $crossings boundary crossings"
echo "smoke[p1-character-traversal]: PASS -- streaming observed $crossings boundary crossings"

bench_line="$(grep '^bench:' "$engine_stdout" | tail -1 || true)"
[[ -n "$bench_line" ]] || fail "bench summary missing"
entities="$(grep -oE 'entities=[0-9]+' <<<"$bench_line" | head -1 | cut -d= -f2)"
: "${entities:=0}"
(( entities >= P1_ENTITY_FLOOR )) || fail "source cell populated only $entities entities"
if grep -Eiq 'panicked at|VUID-|validation error|ERROR.*Vulkan|Vulkan.*ERROR' \
    "$engine_stdout" "$engine_stderr"; then
    fail "panic or Vulkan validation error appeared in engine logs"
fi

echo "$bench_line"
echo "smoke[p1-character-traversal]: PASS -- walk -> door -> boundary -> return -> door"
