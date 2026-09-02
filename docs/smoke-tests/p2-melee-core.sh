#!/usr/bin/env bash
# Playable-slice P2 core combat gate against a frozen actor fixture.
#
# Setup-only `combat.approach` repositions the real character capsule. Every
# hit still enters through `input.press attack` -> ActionBindings/ActionState
# -> camera ray -> actor-owned bone collider -> HitEvent -> Health/death.
#
# Game-parameterised (#3039): the cell, the frozen reference/base pair, the
# weapon family and the target's Health all come from `fixtures/<game>.env`.
# Pass the game as the first argument (or set `BYROREDUX_SMOKE_GAME`);
# default `skyrim_se`.
#
#   docs/smoke-tests/p2-melee-core.sh                   # Skyrim SE
#   docs/smoke-tests/p2-melee-core.sh fnv               # Fallout New Vegas

set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/fixture.sh"
smoke_load_fixture p2-melee-core "$@"
smoke_require_fixture_fields \
    P2_CELL P2_PROBE_CELL_LINE P2_PROBE_NPC_LINE P2_TARGET_NAME \
    P2_TARGET_REFR_LINE P2_TARGET_HEALTH P2_BENCH_FRAMES

ROOT_DIR="$SMOKE_ROOT_DIR"
ENGINE_BIN="$ROOT_DIR/target/release/byroredux"
DEBUG_BIN="$ROOT_DIR/target/release/byro-dbg"
PORT="${BYRO_DEBUG_PORT:-9876}"
BENCH_FRAMES="${BYROREDUX_SMOKE_FRAMES:-$P2_BENCH_FRAMES}"
TIMEOUT="${BYROREDUX_SMOKE_TIMEOUT:-360}"

LOG_DIR="$(mktemp -d /tmp/byro-p2-melee-core.XXXXXX)"
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

# stderr of the engine session currently under test. Reassigned by
# `launch_held_engine` so a failure after the gate-5 relaunch (#3009) tails
# the RELOADED session's log rather than the terminated first one.
current_stderr="$LOG_DIR/engine.stderr"
fail() {
    keep_artifacts=1
    echo "smoke[p2-melee-core]: FAIL -- $*"
    echo "smoke[p2-melee-core]: artifacts retained at $LOG_DIR"
    tail -60 "$current_stderr" 2>/dev/null || true
    exit 1
}

smoke_require_data

if [[ ! -x "$ENGINE_BIN" || ! -x "$DEBUG_BIN" ]]; then
    echo "smoke[p2-melee-core]: building release binaries"
    (cd "$ROOT_DIR" && cargo build --release --quiet -p byroredux -p byro-dbg)
fi

engine_stdout="$LOG_DIR/engine.stdout"
engine_stderr="$LOG_DIR/engine.stderr"
command_log="$LOG_DIR/command.log"
status_log="$LOG_DIR/combat.status"
inventory_log="$LOG_DIR/inventory.entities"
mesh_log="$LOG_DIR/target.mesh"
fixture_log="$LOG_DIR/fixture.preflight"
inventory_status_log="$LOG_DIR/inventory.status"
settings_status_log="$LOG_DIR/settings.status"
player_status_log="$LOG_DIR/player.status"
# Ring slot the gate-5 block below writes. The ring is 10 deep and redirected
# into `$LOG_DIR` above, so this cannot collide with an operator's saves;
# fixtures may still override it.
P2_SAVE_SLOT="${P2_SAVE_SLOT:-9}"
save_log="$LOG_DIR/save.slot"
save_info_log="$LOG_DIR/save.info"
reloaded_stderr="$LOG_DIR/engine.reloaded.stderr"
inventory_reloaded_log="$LOG_DIR/inventory.status.reloaded"

debug_commands() {
    local commands="$1"
    local output="$2"
    env BYRO_DEBUG_PORT="$PORT" "$DEBUG_BIN" >"$output" 2>&1 <<EOF
$commands
.quit
EOF
}

wait_for_pattern() {
    local commands="$1"
    local pattern="$2"
    local output="$3"
    local description="$4"
    local deadline=$(( $(date +%s) + TIMEOUT ))
    while true; do
        if debug_commands "$commands" "$output" && grep -Fq "$pattern" "$output"; then
            echo "smoke[p2-melee-core]: PASS -- $description"
            return 0
        fi
        if (( $(date +%s) > deadline )); then
            cat "$output" 2>/dev/null || true
            fail "timeout waiting for $description ('$pattern')"
        fi
        kill -0 "$engine_pid" 2>/dev/null || fail "engine exited while waiting for $description"
        sleep 0.05
    done
}

echo "================================================================"
echo "  smoke[p2-melee-core]: $FIXTURE_LABEL -- $P2_HEADLINE"
echo "================================================================"

cd "$ROOT_DIR"
cargo run --quiet -p byroredux-plugin --example probe_combat_fixture -- \
    "$SMOKE_DATA/$FIXTURE_ESM" "$P2_CELL" >"$fixture_log" \
    || fail "fixture preflight could not parse $FIXTURE_ESM"
grep -Fq "$P2_PROBE_CELL_LINE" "$fixture_log" \
    || fail "fixture CELL drifted from '$P2_PROBE_CELL_LINE'"
grep -Fq "$P2_PROBE_NPC_LINE" "$fixture_log" \
    || fail "fixture reference/base pair drifted from '$P2_PROBE_NPC_LINE'"
for weapon in "${P2_PROBE_WEAPON_LINES[@]}"; do
    grep -Fq "$weapon" "$fixture_log" \
        || fail "weapon leaf $weapon is absent from the fixture family"
done
echo "smoke[p2-melee-core]: PASS -- frozen CELL/reference/base/weapon family preflight"

# #3009 — the engine runs from the repository root, whose default save root is
# `<cwd>/saves`. Point the ring at the harness log dir so the gate-5 block
# below can save without touching the operator's real quicksaves.
save_dir="$LOG_DIR/saves"
mkdir -p "$save_dir"

# Launch the engine into its held interactive state and block until byro-dbg
# can attach. Extracted (#3009) so the gate-5 block can relaunch the SAME
# invocation with `--load`; the only difference between the two launches must
# be the extra arguments, or the continuity comparison is not comparing the
# same session.
launch_held_engine() {
    local stderr_log="$1"
    shift
    current_stderr="$stderr_log"
    env BYRO_DEBUG_PORT="$PORT" RUST_LOG="${BYROREDUX_SMOKE_LOG:-error}" \
        BYROREDUX_SAVE_DIR="$save_dir" \
        "$ENGINE_BIN" \
        "${SMOKE_ENGINE_ARGS[@]}" \
        --cell "$P2_CELL" \
        --player \
        --radius 1 \
        --bench-frames "$BENCH_FRAMES" \
        --bench-hold \
        "$@" \
        >"$engine_stdout" 2>"$stderr_log" &
    engine_pid=$!

    local deadline=$(( $(date +%s) + TIMEOUT ))
    while ! grep -Fq "bench-hold:" "$stderr_log" 2>/dev/null; do
        if (( $(date +%s) > deadline )); then
            fail "timeout waiting for held engine"
        fi
        kill -0 "$engine_pid" 2>/dev/null || fail "engine exited before bench-hold"
        sleep 0.25
    done
}

launch_held_engine "$engine_stderr"
echo "smoke[p2-melee-core]: PASS -- engine reached held interactive state"

debug_commands "entities Inventory" "$inventory_log" \
    || fail "could not list NPC inventory roots"
mapfile -t candidates < <(
    sed -nE "s/^ *Entity ([0-9]+) \"$P2_TARGET_NAME\".*/\1/p" "$inventory_log"
)
target=""
for candidate in "${candidates[@]}"; do
    debug_commands "mesh.info $candidate" "$mesh_log" || continue
    if grep -Fq "$P2_TARGET_REFR_LINE" "$mesh_log"; then
        target="$candidate"
        break
    fi
done
[[ -n "$target" ]] || fail "frozen reference '$P2_TARGET_REFR_LINE' was not found"
echo "smoke[p2-melee-core]: PASS -- frozen reference resolved to entity $target"

wait_for_pattern "player.status" "mode=Character" "$command_log" "Character mode is active"
wait_for_pattern "combat.status" "attacks=0 hits=0 kills=0" "$status_log" "combat state starts clean"

# #2976 — a swing thrown while holding Block must land (counted, traced) but
# deal zero damage; the pre-fix HitEvent producer hardcoded blocked=false,
# making combat_damage_system's zero-damage arm unreachable from any live
# path. This runs before the real kill sequence below specifically because it
# must NOT change the frozen target's Health — the loop after it still starts
# its expected_hits math from a clean P2_TARGET_HEALTH.
#
# The hold, approach, and swing are queued in one `byro-dbg` connection (a
# frame budget just past what one such batch needs, not a second round-trip)
# so the hold can't lapse between commands while a new debug process spins
# up.
debug_commands "input.hold block 40
combat.approach $target
input.press attack" "$command_log" || fail "could not queue the blocked swing"
grep -Fq "input.hold: queued Block through the C binding for 40 frames" "$command_log" \
    || fail "Block hold did not enter through the normal Block binding"
grep -Fq "input.press: queued action=Attack binding=R" "$command_log" \
    || fail "blocked swing did not enter through the normal Attack binding"
wait_for_pattern "combat.status" "blocking=true attacks=1 hits=1 kills=0" "$status_log" \
    "swing thrown while blocking still lands as a hit"
# #3039 — pin *which* actor the swing landed on. `combat.approach` only
# places the capsule; the swing itself is a camera ray, so in a densely
# populated cell it can land on a bystander standing between the player and
# the approached reference. Without this the Health assertions below fail
# with a misleading "Health changed" message when the real fault is that a
# different actor was hit.
grep -Fq "last_target=$target " "$status_log" \
    || fail "the swing did not land on the fixture's target (entity $target); $(grep -oE 'last_target=[0-9]+' "$status_log" | tail -1)"
grep -Fq "damage=0.0" "$status_log" \
    || fail "a blocked hit must deal zero damage"
grep -Fq "health_before=$P2_TARGET_HEALTH health_after=$P2_TARGET_HEALTH" "$status_log" \
    || fail "a blocked hit must not change the target's Health"
wait_for_pattern "combat.status" "cooldown=0.000" "$status_log" "blocked swing cooldown elapsed"
# There is no console command to release a hold early — wait for the 40-frame
# budget to lapse on its own so the real damage sequence below swings
# unblocked. Without this, a still-active hold silently blocks swing 1 too
# (exactly the failure mode this section exists to catch, just relocated).
wait_for_pattern "combat.status" "blocking=false" "$status_log" \
    "Block hold expired before the real damage sequence"
echo "smoke[p2-melee-core]: PASS -- swing thrown while holding Block landed for zero damage"

debug_commands "inventory.status" "$inventory_status_log" \
    || fail "could not inspect the player's combat loadout"
grep -Fq "Inventory status:" "$inventory_status_log" \
    || fail "inventory.status did not expose the player inventory"
grep -Eq 'player=[0-9]+ stack_rows=[0-9]+ item_count=[0-9]+ occupied_slots=[0-9]+' \
    "$inventory_status_log" \
    || fail "inventory.status did not expose numeric stack/item/equipment state"
loadout_damage="$(sed -nE \
    's/.*equipped_weapon=.* damage=([0-9]+([.][0-9]+)?) source=.*/\1/p' \
    "$inventory_status_log" | tail -1)"
[[ "$loadout_damage" =~ ^[0-9]+([.][0-9]+)?$ ]] \
    || fail "inventory.status did not expose a numeric resolved attack damage"
awk -v damage="$loadout_damage" 'BEGIN { exit !(damage > 0.0) }' \
    || fail "resolved attack damage must be positive (got $loadout_damage)"
echo "smoke[p2-melee-core]: PASS -- combat damage tracks inventory.status ($loadout_damage)"

debug_commands "settings.status" "$settings_status_log" \
    || fail "could not inspect persistent settings state"
grep -Eq 'entries=[1-9][0-9]* persistence_path=.+$' "$settings_status_log" \
    || fail "settings.status did not expose a populated registry and persistence path"
grep -Fq 'controls.bind.attack=key_r restart_required=false' "$settings_status_log" \
    || fail "settings.status did not expose the live Attack binding"
echo "smoke[p2-melee-core]: PASS -- persistent settings registry is observable"

expected_hits="$(awk -v health="$P2_TARGET_HEALTH" -v damage="$loadout_damage" \
    'BEGIN { print int((health + damage - 0.0001) / damage) }')"
previous_health="$P2_TARGET_HEALTH"
# The #2976 blocked swing above already landed one hit (zero damage) before
# this loop starts, so CombatState's cumulative attacks/hits counters are
# offset by one from here on. Health is unaffected, so previous_health still
# correctly starts clean at P2_TARGET_HEALTH.
blocked_swing_count=1
for hit in $(seq 1 "$expected_hits"); do
    debug_commands "combat.approach $target
input.press attack" "$command_log" || fail "could not queue swing $hit"
    grep -Fq "physics_synced=true" "$command_log" \
        || fail "swing $hit did not position the real character body"
    grep -Fq "input.press: queued action=Attack binding=R" "$command_log" \
        || fail "swing $hit did not enter through the normal Attack binding"
    wait_for_pattern \
        "player.status" \
        "grounded=true" \
        "$player_status_log" \
        "swing $hit retained authored floor support"
    wait_for_pattern \
        "combat.status" \
        "hits=$((hit + blocked_swing_count))" \
        "$status_log" \
        "swing $hit emitted one HitEvent and applied damage"
    grep -Fq "damage=$loadout_damage" "$status_log" \
        || fail "swing $hit did not use inventory.status damage $loadout_damage"
    expected_after="$(awk -v before="$previous_health" -v damage="$loadout_damage" \
        'BEGIN { printf "%.1f", before - damage }')"
    grep -Fq "health_before=$previous_health" "$status_log" \
        || fail "swing $hit began from the wrong Health value"
    grep -Fq "health_after=$expected_after" "$status_log" \
        || fail "swing $hit produced the wrong Health result"
    previous_health="$expected_after"
    wait_for_pattern "combat.status" "cooldown=0.000" "$status_log" "swing $hit cooldown elapsed"
done

total_hits=$((expected_hits + blocked_swing_count))
grep -Fq "attacks=$total_hits hits=$total_hits kills=1" "$status_log" \
    || fail "final counters are not exactly $total_hits attacks / $total_hits hits / 1 kill"
grep -Fq "killed=true" "$status_log" || fail "zero Health did not mark the kill"
if [[ -n "${P2_RAGDOLL_BODIES:-}" ]]; then
    grep -Fq "ragdoll activated ($P2_RAGDOLL_BODIES bodies)" "$status_log" \
        || fail "death did not activate the frozen target's $P2_RAGDOLL_BODIES-body ragdoll"
else
    # The fixture has no measured body count yet. Still gate that a ragdoll
    # activated at all — a missing count must not silently drop the check.
    grep -Fq "ragdoll activated (" "$status_log" \
        || fail "death did not activate the frozen target's ragdoll"
    echo "smoke[p2-melee-core]: NOTE -- $SMOKE_GAME.env pins no ragdoll body count yet"
fi

echo "smoke[p2-melee-core]: PASS -- $P2_TARGET_HEALTH Health -> $expected_hits bound attacks at $loadout_damage damage -> Dead -> ragdoll"

# ── Gate 5 (#3009) — inventory/equipment survives save -> exit -> reload ────
#
# `playable-vertical-slice.md` gate 5 requires inventory/equipment state to
# survive save -> PROCESS EXIT -> reload, so this does exactly that: save a
# slot in the live session, terminate the engine, relaunch the identical
# invocation with `--load`, and require the loadout to come back.
#
# The comparison is on the id-FREE half of `inventory.status` only. The reload
# re-runs the cell load in a fresh process, and `EntityId`s are monotonic and
# never recycled (#372), so `player=<id>` legitimately differs across the round
# trip; pinning it would fail on a working engine.
loadout_before="$(sed -nE 's/.*(stack_rows=[0-9]+ item_count=[0-9]+ occupied_slots=[0-9]+).*/\1/p' \
    "$inventory_status_log" | tail -1)"
weapon_before="$(grep -F '  equipped_weapon=' "$inventory_status_log" | tail -1)"
[[ -n "$loadout_before" && -n "$weapon_before" ]] \
    || fail "could not capture the pre-save loadout from inventory.status"

debug_commands "save $P2_SAVE_SLOT" "$save_log" \
    || fail "could not queue a save to slot $P2_SAVE_SLOT"
grep -Fq "saved slot $P2_SAVE_SLOT" "$save_log" \
    || fail "save $P2_SAVE_SLOT did not write a slot (validation gate rejected the world?)"
debug_commands "save.info $P2_SAVE_SLOT" "$save_info_log" \
    || fail "could not verify slot $P2_SAVE_SLOT"
grep -Fq "slot $P2_SAVE_SLOT: VALID" "$save_info_log" \
    || fail "slot $P2_SAVE_SLOT did not decode as a valid container"
# #3552 (RT-6) — no `^` anchor: `byro-dbg` prints `DebugResponse::Value`
# via `serde_json::to_string_pretty` (`tools/byro-dbg/src/display.rs`),
# which renders the whole multi-line save.info dump as ONE JSON-escaped
# line (`\n` stays a literal two-char escape). An anchored match can never
# find "  Inventory: N rows" mid-line, so this gate was permanently red on
# a healthy build. Every other assertion in this script already avoids the
# anchor by using `grep -F`.
grep -Eq '  Inventory: [0-9]+ rows' "$save_info_log" \
    || fail "slot $P2_SAVE_SLOT carries no Inventory column — gate 5 is unassertable"
echo "smoke[p2-melee-core]: PASS -- slot $P2_SAVE_SLOT written and verified with an Inventory column"

# Process exit. `wait` before relaunching so the debug port is free and the
# next `engine_pid` assignment cannot orphan this one past the cleanup trap.
kill -TERM "$engine_pid" 2>/dev/null || true
wait "$engine_pid" 2>/dev/null || true
engine_pid=""

launch_held_engine "$reloaded_stderr" --load "$P2_SAVE_SLOT"
grep -Fq "startup --load" "$reloaded_stderr" \
    || echo "smoke[p2-melee-core]: NOTE -- startup --load produced no log line at RUST_LOG=${BYROREDUX_SMOKE_LOG:-error}"
echo "smoke[p2-melee-core]: PASS -- engine relaunched from slot $P2_SAVE_SLOT"

# The startup load is queued and applied between frames, so poll rather than
# sampling once.
wait_for_pattern "inventory.status" "$loadout_before" "$inventory_reloaded_log" \
    "inventory stacks/equipment survived save -> exit -> reload"
grep -Fq "$weapon_before" "$inventory_reloaded_log" \
    || fail "the equipped weapon did not survive save -> exit -> reload (was '$weapon_before')"
echo "smoke[p2-melee-core]: PASS -- gate 5: $loadout_before + equipped weapon restored in a fresh process"

echo "smoke[p2-melee-core]: PASS"
