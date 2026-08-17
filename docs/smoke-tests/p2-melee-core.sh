#!/usr/bin/env bash
# Playable-slice P2 core combat gate against the frozen Bleak Falls Draugr.
#
# Setup-only `combat.approach` repositions the real character capsule. Every
# hit still enters through `input.press attack` -> ActionBindings/ActionState
# -> camera ray -> actor-owned bone collider -> HitEvent -> Health/death.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
ENGINE_BIN="$ROOT_DIR/target/release/byroredux"
DEBUG_BIN="$ROOT_DIR/target/release/byro-dbg"
SKYRIM_DATA="${BYROREDUX_SKYRIM_DATA:-/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data}"
PORT="${BYRO_DEBUG_PORT:-9876}"
BENCH_FRAMES="${BYROREDUX_SMOKE_FRAMES:-5}"
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

fail() {
    keep_artifacts=1
    echo "smoke[p2-melee-core]: FAIL -- $*"
    echo "smoke[p2-melee-core]: artifacts retained at $LOG_DIR"
    tail -60 "$LOG_DIR/engine.stderr" 2>/dev/null || true
    exit 1
}

for required in \
    "$SKYRIM_DATA/Skyrim.esm" \
    "$SKYRIM_DATA/Skyrim - Meshes0.bsa" \
    "$SKYRIM_DATA/Skyrim - Textures0.bsa" \
    "$SKYRIM_DATA/Skyrim - Misc.bsa"; do
    if [[ ! -f "$required" ]]; then
        echo "smoke[p2-melee-core]: SKIP -- missing $required"
        exit 77
    fi
done

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
echo "  smoke[p2-melee-core]: Bleak Falls Draugr melee death"
echo "================================================================"

cd "$ROOT_DIR"
cargo run --quiet -p byroredux-plugin --example probe_combat_fixture -- \
    "$SKYRIM_DATA/Skyrim.esm" BleakFallsBarrow01 >"$fixture_log" \
    || fail "fixture preflight could not parse Skyrim.esm"
grep -Fq "CELL BleakFallsBarrow01 form=000371DE" "$fixture_log" \
    || fail "fixture CELL drifted from 000371DE"
grep -Fq "NPC ref=000383F7 base=000E9895" "$fixture_log" \
    || fail "fixture reference/base pair drifted from 000383F7/000E9895"
grep -Fq "0001CB64:DraugrBattleAxe:damage=18" "$fixture_log" \
    || fail "Draugr Battleaxe leaf 0001CB64 is absent from the fixture family"
grep -Fq "000236A5:DraugrGreatsword:damage=17" "$fixture_log" \
    || fail "Draugr Greatsword leaf 000236A5 is absent from the fixture family"
echo "smoke[p2-melee-core]: PASS -- frozen CELL/reference/base/weapon family preflight"

env BYRO_DEBUG_PORT="$PORT" RUST_LOG="${BYROREDUX_SMOKE_LOG:-error}" \
    "$ENGINE_BIN" \
    --esm "$SKYRIM_DATA/Skyrim.esm" \
    --cell BleakFallsBarrow01 \
    --bsa "$SKYRIM_DATA/Skyrim - Meshes0.bsa" \
    --textures-bsa "$SKYRIM_DATA/Skyrim - Textures0.bsa" \
    --scripts-bsa "$SKYRIM_DATA/Skyrim - Misc.bsa" \
    --player \
    --radius 1 \
    --bench-frames "$BENCH_FRAMES" \
    --bench-hold \
    >"$engine_stdout" 2>"$engine_stderr" &
engine_pid=$!

deadline=$(( $(date +%s) + TIMEOUT ))
while ! grep -Fq "bench-hold:" "$engine_stderr" 2>/dev/null; do
    if (( $(date +%s) > deadline )); then
        fail "timeout waiting for held engine"
    fi
    kill -0 "$engine_pid" 2>/dev/null || fail "engine exited before bench-hold"
    sleep 0.25
done
echo "smoke[p2-melee-core]: PASS -- engine reached held interactive state"

debug_commands "entities Inventory" "$inventory_log" \
    || fail "could not list NPC inventory roots"
mapfile -t candidates < <(
    sed -nE 's/^ *Entity ([0-9]+) "encdraugr01ambushmelee2hheadm06".*/\1/p' \
        "$inventory_log"
)
target=""
for candidate in "${candidates[@]}"; do
    debug_commands "mesh.info $candidate" "$mesh_log" || continue
    if grep -Fq "REFR FormID:       0x0383F7" "$mesh_log"; then
        target="$candidate"
        break
    fi
done
[[ -n "$target" ]] || fail "frozen reference 000383F7 was not found"
echo "smoke[p2-melee-core]: PASS -- frozen reference 000383F7 resolved to entity $target"

wait_for_pattern "player.status" "mode=Character" "$command_log" "Character mode is active"
wait_for_pattern "combat.status" "attacks=0 hits=0 kills=0" "$status_log" "combat state starts clean"

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

expected_hits="$(awk -v health=50.0 -v damage="$loadout_damage" \
    'BEGIN { print int((health + damage - 0.0001) / damage) }')"
previous_health="50.0"
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
        "hits=$hit" \
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

grep -Fq "attacks=$expected_hits hits=$expected_hits kills=1" "$status_log" \
    || fail "final counters are not exactly $expected_hits attacks / $expected_hits hits / 1 kill"
grep -Fq "killed=true" "$status_log" || fail "zero Health did not mark the kill"
grep -Fq "ragdoll activated (18 bodies)" "$status_log" \
    || fail "death did not activate the frozen Draugr's ragdoll"

echo "smoke[p2-melee-core]: PASS -- 50 Health -> $expected_hits bound attacks at $loadout_damage damage -> Dead -> 18-body ragdoll"
echo "smoke[p2-melee-core]: PASS"
