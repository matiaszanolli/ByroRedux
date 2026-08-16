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
        exit 0
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
env BYRO_DEBUG_PORT="$PORT" RUST_LOG="error" \
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
    if grep -Fq "REFR FormID:       0x0380B4" "$mesh_log"; then
        target="$candidate"
        break
    fi
done
[[ -n "$target" ]] || fail "frozen reference 000380B4 was not found"
echo "smoke[p2-melee-core]: PASS -- frozen reference 000380B4 resolved to entity $target"

wait_for_pattern "player.status" "mode=Character" "$command_log" "Character mode is active"
wait_for_pattern "combat.status" "attacks=0 hits=0 kills=0" "$status_log" "combat state starts clean"

expected_health=(42.0 34.0 26.0 18.0 10.0 2.0 -6.0)
for swing in "${!expected_health[@]}"; do
    hit=$(( swing + 1 ))
    debug_commands "combat.approach $target
input.press attack" "$command_log" || fail "could not queue swing $hit"
    grep -Fq "physics_synced=true" "$command_log" \
        || fail "swing $hit did not position the real character body"
    grep -Fq "queued Attack through the R binding" "$command_log" \
        || fail "swing $hit did not enter through the normal Attack binding"
    wait_for_pattern \
        "combat.status" \
        "hits=$hit" \
        "$status_log" \
        "swing $hit emitted one HitEvent and applied damage"
    grep -Fq "damage=8.0" "$status_log" \
        || fail "swing $hit did not use the unarmed 8-damage rule"
    grep -Fq "health_after=${expected_health[$swing]}" "$status_log" \
        || fail "swing $hit produced the wrong Health result"
    wait_for_pattern "combat.status" "cooldown=0.000" "$status_log" "swing $hit cooldown elapsed"
done

grep -Fq "attacks=7 hits=7 kills=1" "$status_log" \
    || fail "final counters are not exactly 7 attacks / 7 hits / 1 kill"
grep -Fq "killed=true" "$status_log" || fail "zero Health did not mark the kill"
grep -Fq "ragdoll activated (18 bodies)" "$status_log" \
    || fail "death did not activate the frozen Draugr's ragdoll"

echo "smoke[p2-melee-core]: PASS -- 50 Health -> seven bound attacks -> Dead -> 18-body ragdoll"
echo "smoke[p2-melee-core]: PASS"
