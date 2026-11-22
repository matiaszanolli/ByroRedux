#!/usr/bin/env bash
# Owner-run live gate for Oblivion's Block, #7054 (Oblivion skill-use leveling): the player holds Block with a weapon in
# hand (WeapIronBow, 00025231: a weapon blocks 0.5% a level) in Anvil's Count's Arms while the first patron
# `stealth.status` lists is set on them (`StartCombat`, 0x1016). The patron's strikes meet the guard, so `levelup` shows
# Block banked (1.25 a blocked hit) and no longer lists Block as unfed, and Light Armor is fed by the same hits (the
# strikes did land). Bare hands against an armed attacker have no effect (UESP), so the gate wields the bow.
# Requires a Vulkan device, display, and BYROREDUX_OBLIVION_DATA.
set -euo pipefail

GATE=oblivion-block
DATA="${BYROREDUX_OBLIVION_DATA:-}"
if [[ -z "$DATA" || ! -f "$DATA/Oblivion.esm" || ! -f "$DATA/Oblivion - Meshes.bsa" || ! -f "$DATA/Oblivion - Textures - Compressed.bsa" ]]; then
    echo "smoke[$GATE]: SKIP -- missing BYROREDUX_OBLIVION_DATA (set it to the Oblivion Data directory)"
    exit 77
fi
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
DATA="$(cd "$DATA" && pwd)"
cd "$ROOT"
PORT="${BYRO_DEBUG_PORT:-19885}"
TIMEOUT="${BYROREDUX_SMOKE_TIMEOUT:-300}"
LOG_DIR="$(mktemp -d "/tmp/byro-$GATE.XXXXXX")"
engine_pid=""
stop_engine() {
    if [[ -n "$engine_pid" ]]; then
        kill -TERM "$engine_pid" 2>/dev/null || true
        wait "$engine_pid" 2>/dev/null || true
    fi
}
trap stop_engine EXIT
fail() { echo "smoke[$GATE]: FAIL -- $*; artifacts: $LOG_DIR" >&2; exit 1; }
if ss -ltn "sport = :$PORT" | grep -q LISTEN; then
    fail "port $PORT already occupied"
fi
if pgrep -x byroredux >/dev/null; then
    fail 'another engine instance is already running'
fi
cargo build --release --offline --quiet -p byroredux -p byro-dbg || fail 'build failed'
command() {
    timeout "$TIMEOUT" env BYRO_DEBUG_PORT="$PORT" "$ROOT/target/release/byro-dbg" >"$2" 2>&1 <<CMDS
$1
.quit
CMDS
    sed -i 's/\\n/\n/g' "$2"
}
env BYRO_DEBUG_PORT="$PORT" BYRO_DEBUG_SERVER=1 \
    BYROREDUX_SETTINGS_PATH="$LOG_DIR/settings.toml" \
    "$ROOT/target/release/byroredux" --esm "$DATA/Oblivion.esm" --cell AnvilTheCountsArms \
    --bsa "$DATA/Oblivion - Meshes.bsa" --textures-bsa "$DATA/Oblivion - Textures - Compressed.bsa" \
    --player --bench-frames 30 --bench-hold >"$LOG_DIR/engine.log" 2>&1 &
engine_pid=$!
deadline=$((SECONDS + TIMEOUT))
until grep -Fq 'bench-hold:' "$LOG_DIR/engine.log"; do
    kill -0 "$engine_pid" 2>/dev/null || fail 'engine exited before bench-hold'
    (( SECONDS < deadline )) || fail 'timed out waiting for bench-hold'
    sleep 0.25
done

banked() { sed -nE "s/^.*levelup: $2 [0-9]+ banked ([0-9.]+) .*/\1/p" "$1" | head -1; }
BOW=0x00025231

command 'levelup' "$LOG_DIR/before.log" || fail 'levelup failed'
grep -E 'fed skills 16/21; unfed: ' "$LOG_DIR/before.log" | grep -Fq Block && fail 'Block is still reported unfed'
grep -Fq 'fed skills 16/21; unfed: ' "$LOG_DIR/before.log" || fail "levelup did not report 16 of 21 skills fed: $(cat "$LOG_DIR/before.log")"
command 'stealth.status' "$LOG_DIR/stealth.log" || fail 'stealth.status failed'
target="$(sed -nE 's/.*entity ([0-9]+) ref .*/\1/p' "$LOG_DIR/stealth.log" | head -1)"
[[ -n "$target" ]] || fail 'no patron to fight in the cell'

# A weapon in hand, Block held, the patron set on the player.
command "inv.add $BOW 1" "$LOG_DIR/add.log" || fail 'inv.add failed'
command "inv.equip $BOW" "$LOG_DIR/equip.log" || fail 'inv.equip failed'
sleep 1
command 'input.hold block 10000' "$LOG_DIR/hold.log" || fail 'input.hold failed'
command "combat.approach $target
obscript.cmd $target 0x1016 r:0x14" "$LOG_DIR/fight.log" || fail 'the patron could not be set on the player'
got=0
for _ in $(seq 1 20); do
    sleep 1
    command 'levelup' "$LOG_DIR/after.log" || fail 'levelup failed'
    if python3 - "$(banked "$LOG_DIR/after.log" Block)" <<'PY'
import sys
sys.exit(0 if float(sys.argv[1] or 0) >= 1.25 - 1e-6 else 1)
PY
    then got=1; break; fi
done
[[ "$got" == 1 ]] || fail "Block banked nothing from a held guard against the patron's strikes: $(grep -E 'levelup: (Block|LightArmor) ' "$LOG_DIR/after.log")"
skill_value() { sed -nE "s/^.*levelup: $2 ([0-9]+) banked.*/\1/p" "$1" | head -1; }
python3 - "$(banked "$LOG_DIR/after.log" LightArmor)" "$(skill_value "$LOG_DIR/after.log" LightArmor)" <<'PY' || fail 'the strikes did not land (Light Armor gained nothing)'
import sys
sys.exit(0 if float(sys.argv[1] or 0) > 0 or int(sys.argv[2] or 5) > 5 else 1)
PY
printf 'smoke[%s]: PASS -- Block banked %s from the held guard against entity %s; artifacts: %s\n' "$GATE" "$(banked "$LOG_DIR/after.log" Block)" "$target" "$LOG_DIR"
