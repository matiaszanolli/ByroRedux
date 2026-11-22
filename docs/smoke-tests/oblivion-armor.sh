#!/usr/bin/env bash
# Owner-run live gate for Oblivion's armor rating on a weapon hit, the shared hit path (player-archery BOW-DMG-001 and the
# Oblivion ruleset's "Armor Rating"): the player wears the iron cuirass (0001C6D1, a displayed 10, heavy) in Anvil's Count's
# Arms while the first patron `stealth.status` lists is set on them (`StartCombat`, 0x1016). At Heavy Armor 5 the piece is worth
# 10 x (0.35 + 0.0065 x 5) = 3.8, so each landed strike reads `armor 4%` in `combat.status`'s outcome and the damage is below
# the strike's. Requires a Vulkan device, display, and BYROREDUX_OBLIVION_DATA.
set -euo pipefail

GATE=oblivion-armor
DATA="${BYROREDUX_OBLIVION_DATA:-}"
if [[ -z "$DATA" || ! -f "$DATA/Oblivion.esm" || ! -f "$DATA/Oblivion - Meshes.bsa" || ! -f "$DATA/Oblivion - Textures - Compressed.bsa" ]]; then
    echo "smoke[$GATE]: SKIP -- missing BYROREDUX_OBLIVION_DATA (set it to the Oblivion Data directory)"
    exit 77
fi
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
DATA="$(cd "$DATA" && pwd)"
cd "$ROOT"
PORT="${BYRO_DEBUG_PORT:-19887}"
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

CUIRASS=0x0001C6D1

command 'stealth.status' "$LOG_DIR/stealth.log" || fail 'stealth.status failed'
target="$(sed -nE 's/.*entity ([0-9]+) ref .*/\1/p' "$LOG_DIR/stealth.log" | head -1)"
[[ -n "$target" ]] || fail 'no patron to fight in the cell'

command "inv.add $CUIRASS 1" "$LOG_DIR/add.log" || fail 'inv.add failed'
command "inv.equip $CUIRASS" "$LOG_DIR/equip.log" || fail 'inv.equip failed'
sleep 1
command "combat.approach $target
obscript.cmd $target 0x1016 r:0x14" "$LOG_DIR/fight.log" || fail 'the patron could not be set on the player'
got=0
for _ in $(seq 1 25); do
    sleep 1
    command 'combat.status' "$LOG_DIR/status.log" || fail 'combat.status failed'
    if grep -Fq 'armor 4%' "$LOG_DIR/status.log"; then got=1; break; fi
done
[[ "$got" == 1 ]] || fail "no landed strike read 'armor 4%': $(cat "$LOG_DIR/status.log")"
python3 - "$LOG_DIR/status.log" <<'PY' || fail 'the armored hit did not deal less than the strike'
import re, sys
text = open(sys.argv[1]).read()
incoming = float(re.search(r"incoming=([0-9.]+)", text).group(1))
damage = float(re.search(r"damage=([0-9.]+)", text).group(1))
assert 0 < damage < incoming, (incoming, damage)
print(f"incoming {incoming} -> damage {damage}")
PY
printf 'smoke[%s]: PASS -- the iron cuirass stopped 4%% of the patron strike (%s); artifacts: %s\n' "$GATE" "$(grep -oE 'incoming=[0-9.]+ damage=[0-9.]+' "$LOG_DIR/status.log" | head -1)" "$LOG_DIR"
