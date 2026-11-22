#!/usr/bin/env bash
# Owner-run live gate for Oblivion's Resist Normal Weapons, the shared hit path (the Oblivion ruleset's "Armor Rating"): in the
# Dark Brotherhood ghost cell Perennia's ghost carries the ethereal-undead ability, so her `ResistNormalWeapons` reads 100. The
# player's iron longsword (WeapIronLongsword, 00000C0C: a normal weapon) lands and does nothing (`resisted 100%`, Health
# unchanged); their silver longsword (WeapSilverLongsword, 0002521F: the WEAP flag ignores the resistance) hurts her. A cell with
# the ghost alone keeps `combat.status` on the player's own swings. Requires a Vulkan device, display, and BYROREDUX_OBLIVION_DATA.
set -euo pipefail

GATE=oblivion-ghost
DATA="${BYROREDUX_OBLIVION_DATA:-}"
if [[ -z "$DATA" || ! -f "$DATA/Oblivion.esm" || ! -f "$DATA/Oblivion - Meshes.bsa" || ! -f "$DATA/Oblivion - Textures - Compressed.bsa" ]]; then
    echo "smoke[$GATE]: SKIP -- missing BYROREDUX_OBLIVION_DATA (set it to the Oblivion Data directory)"
    exit 77
fi
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
DATA="$(cd "$DATA" && pwd)"
cd "$ROOT"
PORT="${BYRO_DEBUG_PORT:-19888}"
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
    "$ROOT/target/release/byroredux" --esm "$DATA/Oblivion.esm" --cell DarkGhostCell \
    --bsa "$DATA/Oblivion - Meshes.bsa" --textures-bsa "$DATA/Oblivion - Textures - Compressed.bsa" \
    --player --bench-frames 30 --bench-hold >"$LOG_DIR/engine.log" 2>&1 &
engine_pid=$!
deadline=$((SECONDS + TIMEOUT))
until grep -Fq 'bench-hold:' "$LOG_DIR/engine.log"; do
    kill -0 "$engine_pid" 2>/dev/null || fail 'engine exited before bench-hold'
    (( SECONDS < deadline )) || fail 'timed out waiting for bench-hold'
    sleep 0.25
done

IRON=0x00000C0C
SILVER=0x0002521F
health() { command "obscript.cmd $1 GetAV Health" "$LOG_DIR/health-$1-$2.log" || fail 'GetAV Health failed'; sed -nE 's/.*0x[0-9a-f]+ -> (-?[0-9.]+).*/\1/p' "$LOG_DIR/health-$1-$2.log" | head -1; }

sleep 2
command 'find("darkghostperennia")' "$LOG_DIR/find.log" || fail 'find failed'
ghost="$(grep -oE 'Entity [0-9]+' "$LOG_DIR/find.log" | head -1 | cut -d' ' -f2)"
[[ -n "$ghost" ]] || fail "no ghost in the cell: $(cat "$LOG_DIR/find.log")"
command "obscript.cmd $ghost GetAV ResistNormalWeapons" "$LOG_DIR/resist.log" || fail 'GetAV ResistNormalWeapons failed'
[[ "$(sed -nE 's/.*0x[0-9a-f]+ -> (-?[0-9.]+).*/\1/p' "$LOG_DIR/resist.log" | head -1)" == 100 ]] || fail "the ghost does not carry Resist Normal Weapons 100: $(cat "$LOG_DIR/resist.log")"

command "inv.add $IRON 1" "$LOG_DIR/add1.log" || fail 'inv.add failed'
command "inv.add $SILVER 1" "$LOG_DIR/add2.log" || fail 'inv.add failed'
command "inv.equip $IRON" "$LOG_DIR/eq1.log" || fail 'inv.equip failed'
sleep 1
hp0="$(health "$ghost" 0)"
command "combat.approach $ghost" "$LOG_DIR/approach.log" || fail 'combat.approach failed'

# The iron longsword: the swing lands on the ghost and is resisted.
for _ in 1 2 3; do
    command 'input.press attack' "$LOG_DIR/swing-iron.log" || fail 'input.press attack failed'
    sleep 1
done
command 'combat.status' "$LOG_DIR/status-iron.log" || fail 'combat.status failed'
grep -Fq "last_target=$ghost" "$LOG_DIR/status-iron.log" || fail "the swing did not land on the ghost: $(cat "$LOG_DIR/status-iron.log")"
grep -Fq 'resisted 100%' "$LOG_DIR/status-iron.log" || fail "the iron blow was not resisted: $(cat "$LOG_DIR/status-iron.log")"
hp1="$(health "$ghost" 1)"
python3 - "$hp0" "$hp1" <<'PY' || fail 'a normal weapon hurt the ghost'
import sys
assert abs(float(sys.argv[1]) - float(sys.argv[2])) < 1e-3, sys.argv
PY

# The silver longsword: the flag ignores the resistance and the ghost loses Health.
command "inv.equip $SILVER" "$LOG_DIR/eq2.log" || fail 'inv.equip failed'
sleep 1
for _ in 1 2; do
    command 'input.press attack' "$LOG_DIR/swing-silver.log" || fail 'input.press attack failed'
    sleep 1
done
hp2="$(health "$ghost" 2)"
python3 - "$hp1" "$hp2" <<'PY' || fail 'a silver weapon did not hurt the ghost'
import sys
assert float(sys.argv[2]) < float(sys.argv[1]), sys.argv
PY
printf 'smoke[%s]: PASS -- the ghost (Resist Normal Weapons 100) took nothing from an iron longsword (Health %s) and lost Health to a silver one (%s -> %s); artifacts: %s\n' "$GATE" "$hp1" "$hp1" "$hp2" "$LOG_DIR"
