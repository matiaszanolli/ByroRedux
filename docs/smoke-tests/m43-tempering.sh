#!/usr/bin/env bash
# M43 gate — improving (tempering) a held item at a grindstone, live (Skyrim SE).
#
#   1. Boot the Skyrim SE interior `HaltedStreamCamp01` (it places a sharpening wheel, base 0x6E9C2,
#      `CraftingBlacksmithSharpeningWheel`) with a player and walk the player onto the wheel, whose position
#      `craft benches` reads from the running engine.
#   2. Give the player a Steel Sword and Steel Ingots. `craft list` offers `Improve Steel Sword: unimproved
#      -> <quality>` — the improving recipe (COBJ `TemperWeaponSteelSword`, bench keyword
#      CraftingSmithingSharpeningWheel), listed only because the player holds the sword.
#   3. `craft make` improves it IN PLACE: one ingot is spent, no second sword appears (the old behaviour
#      handed out a copy), and the list now says there is no better quality at this Smithing level.
set -euo pipefail

PORT="${BYRO_DEBUG_PORT:-19883}"
TIMEOUT="${BYROREDUX_SMOKE_TIMEOUT:-300}"
SWORD="${BYRO_SMOKE_SWORD:-0x00013989}"
INGOT="${BYRO_SMOKE_INGOT:-0x0005ACE5}"
RECIPE="${BYRO_SMOKE_TEMPER_RECIPE:-0x000F46AE}"
CELL="${BYRO_SMOKE_TEMPER_CELL:-HaltedStreamCamp01}"
WHEEL_BASE="${BYRO_SMOKE_WHEEL_BASE:-0006E9C2}"
LOG_DIR="$(mktemp -d /tmp/byro-m43-tempering.XXXXXX)"
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
    echo "smoke[m43-tempering]: FAIL -- $*" >&2
    echo "artifacts: $LOG_DIR" >&2
    exit 1
}

# #5427 — SKIP != PASS: a runner without the game's data measures nothing.
DATA="${BYROREDUX_SKYRIMSE_DATA:-/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data}"
for f in "Skyrim.esm" "Skyrim - Meshes0.bsa" "Skyrim - Textures0.bsa"; do
    if [ ! -f "$DATA/$f" ]; then
        echo "smoke[m43-tempering]: SKIP -- missing $DATA/$f"
        exit 77
    fi
done

if ss -ltn "sport = :$PORT" | grep -q LISTEN; then
    fail "port $PORT already occupied; refusing to attach to another session"
fi

cargo build --release --offline --quiet -p byroredux -p byro-dbg \
    || fail "release build failed"

wait_log() {
    local file="$1" pattern="$2" deadline=$((SECONDS + TIMEOUT))
    until grep -Eq "$pattern" "$file"; do
        kill -0 "$engine_pid" 2>/dev/null || fail "engine exited waiting for $pattern"
        (( SECONDS < deadline )) || fail "timeout waiting for $pattern"
        sleep 0.5
    done
}

debug_command() {
    timeout 60 env BYRO_DEBUG_PORT="$PORT" target/release/byro-dbg >"$2" 2>&1 <<EOF2
$1
.quit
EOF2
    sed -i 's/\\n/\n/g' "$2"
}

env BYRO_DEBUG_PORT="$PORT" BYRO_DEBUG_SERVER=1 \
    BYROREDUX_SETTINGS_PATH="$LOG_DIR/settings.toml" \
    RUST_LOG="warn,byroredux::asset_provider=info" \
    target/release/byroredux \
    --game skyrim_se --cell "$CELL" \
    --player --bench-frames 30 --bench-hold \
    >"$LOG_DIR/session.stdout" 2>"$LOG_DIR/session.stderr" &
engine_pid=$!

wait_log "$LOG_DIR/session.stderr" 'Installed [0-9]+ crafting recipes'
wait_log "$LOG_DIR/session.stderr" 'bench-hold:'
sleep 10

debug_command "craft benches" "$LOG_DIR/benches.log" || fail "craft unavailable"
wheel="$(grep -E "^bench [0-9]+ base $WHEEL_BASE " "$LOG_DIR/benches.log" | head -1 || true)"
[[ -n "$wheel" ]] || fail "no sharpening wheel is loaded in $CELL: $(sed -n '1,8p' "$LOG_DIR/benches.log")"
WHEEL_POS="$(sed -E 's/.* at \(([-0-9.]+), ([-0-9.]+), ([-0-9.]+)\).*/\1 \2 \3/' <<<"$wheel")"
debug_command "cam.pos $WHEEL_POS" "$LOG_DIR/move.log" || fail "cam.pos unavailable"
sleep 3
debug_command "inv.add $SWORD 1" "$LOG_DIR/add-sword.log" || fail "inv.add unavailable"
debug_command "inv.add $INGOT 3" "$LOG_DIR/add-ingot.log" || fail "inv.add unavailable"

debug_command "craft list" "$LOG_DIR/list.log" || fail "craft unavailable"
grep -Fq 'Improve Steel Sword' "$LOG_DIR/list.log" \
    || fail "the improving recipe is not offered at the wheel: $(sed -n '1,12p' "$LOG_DIR/list.log")"

debug_command "inventory.status" "$LOG_DIR/inventory-before.log" || fail "inventory.status unavailable"
before="$(grep -oE 'item_count=[0-9]+' "$LOG_DIR/inventory-before.log" | head -1 | cut -d= -f2)"
debug_command "craft make $RECIPE" "$LOG_DIR/make.log" || fail "craft unavailable"
grep -Fq 'Improved Steel Sword to' "$LOG_DIR/make.log" \
    || fail "the improvement was refused: $(sed -n '1,6p' "$LOG_DIR/make.log")"

debug_command "inventory.status" "$LOG_DIR/inventory.log" || fail "inventory.status unavailable"
after="$(grep -oE 'item_count=[0-9]+' "$LOG_DIR/inventory.log" | head -1 | cut -d= -f2)"
# One ingot spent and nothing made: the item count drops by exactly one (the old behaviour added a sword).
[[ -n "$before" && -n "$after" && $((before - after)) -eq 1 ]] \
    || fail "improving should spend one ingot and make nothing (item_count $before -> $after)"

debug_command "craft list" "$LOG_DIR/list-after.log" || fail "craft unavailable"
grep -Fq 'no better quality' "$LOG_DIR/list-after.log" \
    || fail "the one sword should be at its best quality now: $(sed -n '1,12p' "$LOG_DIR/list-after.log")"

if grep -Fq 'panicked' "$LOG_DIR/session.stderr"; then
    fail "the engine panicked: $(grep -F 'panicked' "$LOG_DIR/session.stderr" | head -1)"
fi

stop_engine
echo "smoke[m43-tempering]: PASS -- the held Steel Sword was improved in place at the grindstone"
echo "($(grep -o 'Improved Steel Sword to [A-Za-z]*' "$LOG_DIR/make.log" | head -1); item count $before -> $after: one ingot, no copy)."
echo "artifacts: $LOG_DIR"
