#!/usr/bin/env bash
# WATAL W1 gate — real-character water traversal.
#
# Closes gate 2 of the water focus in
# `docs/engine/playable-vertical-slice.md`: *"a character can enter, swim
# horizontally and vertically, float/clamp at the surface, exit onto land, and
# cross a water-adjacent cell boundary without falling, sticking, or losing
# input. Camera waterline hysteresis must not strobe."*
#
# W0 (`m-exteriors.sh water`) froze the *rendered* waterline from a fly camera.
# This gate drives the real `CharacterController` capsule through the same
# authored water with held gameplay input: movement flows through
# ActionBindings -> ActionState -> Rapier KCC, and `input.look` writes the same
# yaw/pitch accumulator mouse look owns — which is also the dive control
# (WATAL §9 Q3, OpenMW `movementsolver.cpp:161-165`).
#
# Game-parameterised like the other slice gates: every coordinate comes from
# `fixtures/<game>.env`. Pass the game as the first argument (or set
# `BYROREDUX_SMOKE_GAME`); default `skyrim_se`.
#
#   docs/smoke-tests/w1-water-traversal.sh          # Skyrim SE — authored river
#   docs/smoke-tests/w1-water-traversal.sh fnv      # FNV — Lake Mead CELL water

set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/fixture.sh"
smoke_load_fixture w1-water-traversal "$@"

# A title only gets a W1 route once one has been *measured* on its water — a
# walkable shore, reachable swimmable depth, and a cell edge the capsule can
# actually cross. A fixture without one is an explicit SKIP, never a pass and
# never a weakened route: see the Skyrim note in `fixtures/skyrim_se.env`.
if [[ -z "${W1_WATER_SOURCE:-}" ]]; then
    echo "smoke[w1-water-traversal]: SKIP -- missing W1 water fixture for $SMOKE_GAME"
    echo "  $SMOKE_GAME.env declares no W1_* route; see its W1 section for why."
    exit 77
fi

smoke_require_fixture_fields \
    W1_HEADLINE W1_WORLDSPACE W1_GRID W1_SHORE_POS W1_SHORE_FORWARD \
    W1_ENTER_ACTION W1_ENTER_YAW W1_EXIT_ACTION W1_WATER_SOURCE \
    W1_DIVE_DEPTH W1_ENTITY_FLOOR W1_MAX_WATERLINE_TRANSITIONS

# Water bodies differ in what they can physically gate, and the fixture says
# which — never the script silently. `W1_HEAD_SUBMERSION=0` declares water too
# shallow to put a 128 BU capsule's head under: Skyrim's authored White River
# is ~96 BU deep bed-to-surface, so `head_submerged` (AABB top below the
# surface, i.e. depth >= half_span = 64) and the camera waterline are
# unreachable there by authored geometry, not by an engine defect. Those two
# sub-gates then run only on the deep profile. Everything else — enter, both
# swim axes, the surface clamp, the exit, the boundary — is gated on every
# profile.
W1_HEAD_SUBMERSION="${W1_HEAD_SUBMERSION:-1}"

ROOT_DIR="$SMOKE_ROOT_DIR"
ENGINE_BIN="$ROOT_DIR/target/release/byroredux"
DEBUG_BIN="$ROOT_DIR/target/release/byro-dbg"
PORT="${BYRO_DEBUG_PORT:-9876}"
BENCH_FRAMES="${BYROREDUX_SMOKE_FRAMES:-30}"
TIMEOUT="${BYROREDUX_SMOKE_TIMEOUT:-360}"
RADIUS="${W1_RADIUS:-1}"

LOG_DIR="$(mktemp -d /tmp/byro-w1-water.XXXXXX)"
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

pass() { echo "smoke[w1-water-traversal]: PASS -- $*"; }

fail() {
    keep_artifacts=1
    echo "smoke[w1-water-traversal]: FAIL -- $*"
    echo "smoke[w1-water-traversal]: artifacts retained at $LOG_DIR"
    tail -60 "$LOG_DIR/engine.stderr" 2>/dev/null || true
    exit 1
}

smoke_require_data

if [[ ! -x "$ENGINE_BIN" || ! -x "$DEBUG_BIN" ]]; then
    echo "smoke[w1-water-traversal]: building release binaries"
    (cd "$ROOT_DIR" && cargo build --release --quiet -p byroredux -p byro-dbg)
fi

engine_stdout="$LOG_DIR/engine.stdout"
engine_stderr="$LOG_DIR/engine.stderr"
status_log="$LOG_DIR/player.status"
command_log="$LOG_DIR/command.log"
water_log="$LOG_DIR/water.log"

debug_command() {
    local command="$1"
    local output="$2"
    env BYRO_DEBUG_PORT="$PORT" "$DEBUG_BIN" >"$output" 2>&1 <<EOF
$command
.quit
EOF
}

# Retained for the failure artifact: every water read this gate made.
capture_water() {
    local label="$1"
    {
        echo "── $label ──"
        debug_command "water.dump" "$LOG_DIR/water.tmp" && sed 's/\\n/\n/g' "$LOG_DIR/water.tmp"
        debug_command "water.contacts" "$LOG_DIR/water.tmp" && sed 's/\\n/\n/g' "$LOG_DIR/water.tmp"
        debug_command "player.status" "$LOG_DIR/water.tmp" && sed 's/\\n/\n/g' "$LOG_DIR/water.tmp"
    } >>"$water_log" 2>&1
}

wait_for_debug_pattern() {
    local command="$1" pattern="$2" output="$3" description="$4"
    local deadline=$(( $(date +%s) + TIMEOUT ))
    while true; do
        if debug_command "$command" "$output" && grep -Fq "$pattern" "$output"; then
            pass "$description"
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

wait_for_engine_log() {
    local pattern="$1" description="$2"
    local deadline=$(( $(date +%s) + TIMEOUT ))
    while ! grep -Fq "$pattern" "$engine_stderr" 2>/dev/null; do
        if (( $(date +%s) > deadline )); then
            fail "timeout waiting for $description ('$pattern')"
        fi
        kill -0 "$engine_pid" 2>/dev/null || fail "engine exited while waiting for $description"
        sleep 0.25
    done
    pass "$description"
}

status_field() {
    local field="$1"
    debug_command "player.status" "$status_log" || return 1
    sed -nE "s/.*${field}=([-0-9.]+).*/\1/p" "$status_log" | tail -1
}

status_flag() {
    local field="$1"
    debug_command "player.status" "$status_log" || return 1
    sed -nE "s/.*${field}=(true|false).*/\1/p" "$status_log" | tail -1
}

body_coordinate() {
    local axis="$1"
    debug_command "player.status" "$status_log" || return 1
    case "$axis" in
        x) sed -nE 's/.*body=\(([-0-9.]+),.*/\1/p' "$status_log" | tail -1 ;;
        y) sed -nE 's/.*body=\([-0-9.]+, ([-0-9.]+),.*/\1/p' "$status_log" | tail -1 ;;
        z) sed -nE 's/.*body=\([-0-9.]+, [-0-9.]+, ([-0-9.]+)\).*/\1/p' "$status_log" | tail -1 ;;
        *) return 1 ;;
    esac
}

numeric() { [[ "$1" =~ ^-?[0-9]+([.][0-9]+)?$ ]]; }

# Queue one bounded gameplay hold and wait for the engine to consume it. Unlike
# the P1 helper this never waits on `grounded=true`: a swimmer is deliberately
# never grounded (reference `movementsolver.cpp:379`), so a grounded settle
# would deadlock the moment the capsule leaves the shore.
run_hold() {
    local action="$1" frames="$2" description="$3"
    debug_command "input.hold $action $frames" "$command_log" \
        || fail "could not queue $description"
    grep -Fq "for $frames frames" "$command_log" \
        || fail "$description did not enter through input.hold"

    local start_deadline=$(( $(date +%s) + 10 )) remaining=""
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
    wait_for_debug_pattern "player.status" "input_hold_frames_remaining=0" \
        "$status_log" "$description completed"
}

# Hold `action` in bounded chunks until `predicate` succeeds. The predicate is
# a shell function name so each caller states its own completion condition in
# engine terms (swim verdict, depth, grid) rather than a frame count.
hold_until() {
    local action="$1" predicate="$2" description="$3" chunk="${4:-60}" attempts="${5:-20}"
    local attempt
    for attempt in $(seq 1 "$attempts"); do
        if "$predicate"; then
            pass "$description"
            return 0
        fi
        run_hold "$action" "$chunk" "$description segment $attempt"
    done
    "$predicate" && { pass "$description"; return 0; }
    capture_water "$description (failed)"
    fail "$description never satisfied '$predicate'"
}

is_swimming()     { [[ "$(status_flag swimming)" == true ]]; }
is_not_swimming() { [[ "$(status_flag swimming)" == false ]]; }
is_grounded()     { [[ "$(status_flag grounded)" == true ]]; }

reached_dive_depth() {
    local depth
    depth="$(status_field depth)" || return 1
    numeric "$depth" || return 1
    awk -v depth="$depth" -v target="$W1_DIVE_DEPTH" 'BEGIN { exit !(depth >= target) }'
}

exited_onto_land() {
    is_not_swimming && is_grounded
}

reached_target_grid() {
    debug_command "player.status" "$status_log" || return 1
    grep -Fq "$W1_BOUNDARY_GRID" "$status_log"
}

echo "================================================================"
echo "  smoke[w1-water-traversal]: $FIXTURE_LABEL -- $W1_HEADLINE"
echo "================================================================"

cd "$ROOT_DIR"
env BYRO_DEBUG_PORT="$PORT" \
    RUST_LOG="error,byroredux::systems::water=info,byroredux::app_step=info,byroredux::streaming=info" \
    "$ENGINE_BIN" \
    "${SMOKE_ENGINE_ARGS[@]}" \
    --grid "$W1_GRID" \
    --wrld "$W1_WORLDSPACE" \
    --radius "$RADIUS" \
    --player \
    --camera-pos "$W1_SHORE_POS" \
    --camera-forward "$W1_SHORE_FORWARD" \
    --bench-frames "$BENCH_FRAMES" \
    --bench-hold \
    >"$engine_stdout" 2>"$engine_stderr" &
engine_pid=$!

wait_for_engine_log "bench-hold:" "engine reached the held interactive state"
wait_for_debug_pattern "player.status" "mode=Character" "$status_log" "Character mode is active"
wait_for_debug_pattern "player.status" "grounded=true" "$status_log" "shore spawn settled on land"
[[ "$(status_flag swimming)" == false ]] \
    || fail "the fixture spawned the player already in the water — it must start on land"
pass "shore spawn is dry"
capture_water "spawn"

# The authored water this gate is about must actually be the water we reach:
# W0 pinned the same WATR source form from the fly camera.
debug_command "water.dump" "$command_log" || fail "water.dump failed"
sed 's/\\n/\n/g' "$command_log" | grep -Fq "$W1_WATER_SOURCE" \
    || fail "grid $W1_GRID does not carry the frozen WATR $W1_WATER_SOURCE"
pass "canonical WATR $W1_WATER_SOURCE is resident"

# Freeze the look axis before moving: a windowed smoke still receives
# compositor mouse motion, and here pitch is a *movement* input, not just a
# camera angle — drift would silently turn a level swim into a dive.
debug_command "input.look $W1_ENTER_YAW 0" "$command_log" || fail "could not seed entry look"
grep -Fq "pitch=0.0°" "$command_log" || fail "entry look was not applied"

# ── enter ────────────────────────────────────────────────────────────────
hold_until "$W1_ENTER_ACTION" is_swimming "walked off the shore into the water" \
    "${W1_ENTER_CHUNK:-60}" "${W1_ENTER_ATTEMPTS:-25}"
capture_water "entered"
[[ "$(status_flag grounded)" == false ]] \
    || fail "a swimmer must not report ground contact (reference movementsolver.cpp:379)"
pass "swim state replaced ground contact"
debug_command "water.contacts" "$command_log" || fail "water.contacts failed"
player_entity="$(grep -oE 'player=[0-9]+' "$status_log" | tail -1 | cut -d= -f2)"
[[ -n "$player_entity" ]] || fail "player.status did not report an entity id"
sed 's/\\n/\n/g' "$command_log" | grep -Eq "entity=$player_entity .*fraction=0[.][0-9]*[1-9]" \
    || fail "the kinematic player did not reach the canonical WaterContact sink"
pass "player entity $player_entity is published to water.contacts"

# ── swim down (the look axis is the dive control) ────────────────────────
debug_command "input.look ${W1_DIVE_YAW:-$W1_ENTER_YAW} ${W1_DIVE_PITCH:--60}" "$command_log" \
    || fail "could not aim the dive"
hold_until forward reached_dive_depth "vertical swim descended to depth >= $W1_DIVE_DEPTH" \
    "${W1_DIVE_CHUNK:-45}" "${W1_DIVE_ATTEMPTS:-20}"
capture_water "submerged"
if (( W1_HEAD_SUBMERSION != 0 )); then
    wait_for_debug_pattern "player.status" "submerged=true" "$status_log" \
        "the canonical head-submerged flag is set at depth"
    wait_for_engine_log "submersion: ENTER underwater" "camera waterline entered underwater"
else
    # Shallow water: the descent itself is still gated (above), and it is the
    # only evidence available here — `W1_DIVE_DEPTH` on such a fixture is set
    # past the *passive* float depth (`half_span * 0.35`), so reaching it
    # proves the swim input drove the capsule below where buoyancy alone
    # parks it.
    pass "shallow profile: descent gated on depth alone (no head submersion available)"
fi

# ── surface: the clamp must hold the swimmer in the water ────────────────
debug_command "input.look ${W1_CLIMB_YAW:-$W1_ENTER_YAW} ${W1_CLIMB_PITCH:-60}" "$command_log" \
    || fail "could not aim the ascent"
run_hold forward "${W1_CLIMB_FRAMES:-240}" "sustained upward swim into the surface"
capture_water "surfaced"
surface_swimming="$(status_flag swimming)"
surface_grounded="$(status_flag grounded)"
body_y="$(body_coordinate y)"
numeric "$body_y" || fail "could not read the body height after surfacing"
# Reference `movementsolver.cpp:205-213` — *"don't allow to swim up into the
# air"*. Holding forward at a hard upward pitch for four seconds is exactly the
# input that launched the capsule out of the lake before the clamp existed.
[[ "$surface_swimming" == true ]] \
    || fail "the swimmer left the water by swimming upward (surface clamp lost); body y=$body_y"
[[ "$surface_grounded" == false ]] \
    || fail "the surfaced swimmer reported ground contact at y=$body_y"
pass "surface clamp held the swimmer at the waterline (y=$body_y)"

# ── exit onto land ───────────────────────────────────────────────────────
run_exit_leg() {
    debug_command "input.look ${W1_EXIT_YAW:-$W1_ENTER_YAW} 0" "$command_log" \
        || fail "could not aim the shore exit"
    hold_until "$W1_EXIT_ACTION" exited_onto_land "swam back and stood up on land" \
        "${W1_EXIT_CHUNK:-60}" "${W1_EXIT_ATTEMPTS:-30}"
    capture_water "exited"
    local exit_velocity
    exit_velocity="$(status_field vertical_velocity)"
    numeric "$exit_velocity" || fail "could not read vertical_velocity after the exit"
    # Reference `movementsolver.cpp:427-428` — inertia is zeroed on every
    # below-swimlevel frame, so a water exit starts from rest. A residual
    # buoyancy spring would show up here as a launch or a phantom descent.
    awk -v v="$exit_velocity" -v limit="${W1_MAX_EXIT_VELOCITY:-1.0}" \
        'BEGIN { exit !(v <= limit && v >= -limit) }' \
        || fail "water exit carried vertical velocity $exit_velocity (buoyancy spring leaked into gravity)"
    pass "water exit started from rest (vertical_velocity=$exit_velocity)"
}

# ── water-adjacent cell boundary ─────────────────────────────────────────
#
# The gate asks for the crossing to happen "without falling, sticking, or
# losing input" — not for it to happen on dry land. On this fixture the route
# back along the entry axis re-enters the lake before the cell edge, so the
# boundary is crossed swimming, which is the harder case: the streaming
# reconcile runs while the capsule's support is a water volume rather than a
# collider.
run_boundary_leg() {
    if [[ -z "${W1_BOUNDARY_ACTION:-}" || -z "${W1_BOUNDARY_GRID:-}" ]]; then
        return 0
    fi
    local boundary_swimming boundary_grounded boundary_velocity crossings
    debug_command "input.look ${W1_BOUNDARY_YAW:-$W1_ENTER_YAW} 0" "$command_log" \
        || fail "could not aim the boundary walk"
    hold_until "$W1_BOUNDARY_ACTION" reached_target_grid \
        "crossed the water-adjacent cell boundary to $W1_BOUNDARY_GRID" \
        "${W1_BOUNDARY_CHUNK:-90}" "${W1_BOUNDARY_ATTEMPTS:-30}"
    capture_water "boundary"
    boundary_swimming="$(status_flag swimming)"
    boundary_grounded="$(status_flag grounded)"
    boundary_velocity="$(status_field vertical_velocity)"
    numeric "$boundary_velocity" || fail "could not read vertical_velocity at the boundary"
    [[ "$boundary_swimming" == true || "$boundary_grounded" == true ]] \
        || fail "the boundary crossing left the player neither swimming nor grounded \
(vertical_velocity=$boundary_velocity) — that is the fall this gate exists to catch"
    # A swimmer's vertical velocity is bounded by the swim integrator's own
    # ±120 clamp; a capsule that lost its support crosses that within a few
    # frames of free fall.
    awk -v v="$boundary_velocity" -v limit="${W1_MAX_BOUNDARY_VELOCITY:-130}" \
        'BEGIN { exit !(v <= limit && v >= -limit) }' \
        || fail "the player was falling through the boundary (vertical_velocity=$boundary_velocity)"
    pass "boundary crossing kept support (swimming=$boundary_swimming grounded=$boundary_grounded)"
    crossings="$(grep -Fc "Player crossed cell boundary" "$engine_stderr" || true)"
    (( crossings >= 1 )) || fail "streaming telemetry reported no boundary crossing"
    pass "streaming observed $crossings boundary crossings"
}

# Which side of the exit the crossing falls on is a property of the authored
# shoreline, not of the engine, so the fixture declares it. FNV's beach has dry
# land continuing past the cell edge, so walking out and then on crosses it
# with the water behind you; Skyrim's river valley has its walkable bank in the
# *next* cell downstream, so there the crossing happens while still swimming
# and the exit lands on the far bank.
case "${W1_BOUNDARY_PHASE:-after-exit}" in
    after-exit)
        run_exit_leg
        run_boundary_leg
        ;;
    before-exit)
        run_boundary_leg
        run_exit_leg
        ;;
    *)
        fail "unknown W1_BOUNDARY_PHASE '${W1_BOUNDARY_PHASE}' \
(expected 'after-exit' or 'before-exit')"
        ;;
esac

# Input must still reach the player after the whole route — one more bounded
# hold that the engine has to consume proves the action pipeline is alive.
run_hold "$W1_EXIT_ACTION" 30 "post-route input liveness"

# ── waterline hysteresis must not strobe ─────────────────────────────────
enters="$(grep -Fc "submersion: ENTER underwater" "$engine_stderr" || true)"
exits="$(grep -Fc "submersion: EXIT underwater" "$engine_stderr" || true)"
transitions=$(( enters + exits ))
(( transitions <= W1_MAX_WATERLINE_TRANSITIONS )) \
    || fail "camera waterline strobed: $enters enters + $exits exits > $W1_MAX_WATERLINE_TRANSITIONS"
if (( W1_HEAD_SUBMERSION != 0 )); then
    (( enters >= 1 )) \
        || fail "the camera never went underwater — the dive leg did not submerge the head"
    pass "camera waterline transitions stayed bounded ($enters enters, $exits exits)"
else
    # The upper bound still applies: a strobing hysteresis on water that only
    # ever reaches the chest would be just as wrong, and would show up here as
    # transitions on a camera that never legitimately submerged.
    (( transitions == 0 )) \
        || fail "shallow water produced $transitions waterline transitions — the camera cannot \
legitimately submerge on this profile, so any transition is a hysteresis defect"
    pass "shallow profile: the camera waterline never triggered, as authored depth requires"
fi

bench_line="$(grep '^bench:' "$engine_stdout" | tail -1 || true)"
[[ -n "$bench_line" ]] || fail "bench summary missing"
entities="$(grep -oE 'entities=[0-9]+' <<<"$bench_line" | head -1 | cut -d= -f2)"
: "${entities:=0}"
(( entities >= W1_ENTITY_FLOOR )) || fail "water grid populated only $entities entities"
if grep -Eiq 'panicked at|VUID-|validation error|ERROR.*Vulkan|Vulkan.*ERROR' \
    "$engine_stdout" "$engine_stderr"; then
    fail "panic or Vulkan validation error appeared in engine logs"
fi

echo "$bench_line"
pass "shore -> enter -> dive -> surface -> exit -> boundary"
