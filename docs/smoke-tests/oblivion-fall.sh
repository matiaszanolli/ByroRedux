#!/usr/bin/env bash
# Owner-run live gate for Oblivion's fall damage, #7056 (Oblivion skill-use leveling), the NEGATIVE half: in Anvil's Count's
# Arms a jump and a 300-unit drop through `cam.pos` (a teleport: a body that has not fallen from a height it stood at) cost
# the player no Health and feed no fall experience, and the controller's fall clock raises no error. The POSITIVE half
# (a drop from a ledge costs 80 Health a second of falling times the Acrobatics term and feeds Acrobatics 3.0) needs a ledge
# the cell does not have; it is covered by `a_damaging_landing_costs_health_and_feeds_acrobatics` and the `FallTracker` tests.
# Requires a Vulkan device, display, and BYROREDUX_OBLIVION_DATA.
set -euo pipefail

GATE=oblivion-fall
DATA="${BYROREDUX_OBLIVION_DATA:-}"
if [[ -z "$DATA" || ! -f "$DATA/Oblivion.esm" || ! -f "$DATA/Oblivion - Meshes.bsa" || ! -f "$DATA/Oblivion - Textures - Compressed.bsa" ]]; then
    echo "smoke[$GATE]: SKIP -- missing BYROREDUX_OBLIVION_DATA (set it to the Oblivion Data directory)"
    exit 77
fi
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
DATA="$(cd "$DATA" && pwd)"
cd "$ROOT"
PORT="${BYRO_DEBUG_PORT:-19886}"
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
skill_value() { sed -nE "s/^.*levelup: $2 ([0-9]+) banked.*/\1/p" "$1" | head -1; }
health() { command 'obscript.cmd 0x14 GetAV Health' "$LOG_DIR/health-$1.log" || fail 'GetAV Health failed'; sed -nE 's/.*0x[0-9a-f]+ -> (-?[0-9.]+).*/\1/p' "$LOG_DIR/health-$1.log" | head -1; }

sleep 3
command 'levelup' "$LOG_DIR/before.log" || fail 'levelup failed'
hp_before="$(health before)"
# A jump: Acrobatics 0.3 (usage 0), no damage.
command 'input.press jump' "$LOG_DIR/jump.log" || fail 'input.press jump failed'
sleep 2
# A 300-unit drop through a teleport: the body appears above the floor it stood on and falls back to it.
command 'player.status' "$LOG_DIR/status.log" || fail 'player.status failed'
cam="$(sed -nE 's/.*camera=\(([-0-9.]+), ([-0-9.]+), ([-0-9.]+)\).*/\1 \2 \3/p' "$LOG_DIR/status.log" | head -1)"
if [[ -z "$cam" ]]; then
    body="$(sed -nE 's/.*body=\(([-0-9.]+), ([-0-9.]+), ([-0-9.]+)\).*/\1 \2 \3/p' "$LOG_DIR/status.log" | head -1)"
    [[ -n "$body" ]] || fail "no body position: $(cat "$LOG_DIR/status.log")"
    cam="$(python3 -c "import sys; x,y,z=map(float,sys.argv[1:]); print(x, y+58, z)" $body)"
fi
read -r cx cy cz <<<"$cam"
command "cam.pos $cx $(python3 -c "print($cy + 300)") $cz" "$LOG_DIR/drop.log" || fail 'cam.pos failed'
sleep 4
command 'levelup' "$LOG_DIR/after.log" || fail 'levelup failed'
hp_after="$(health after)"
python3 - "$hp_before" "$hp_after" <<'PY' || fail "the player lost Health to a jump and a teleport drop"
import sys
assert abs(float(sys.argv[1]) - float(sys.argv[2])) < 1e-3, sys.argv
PY
grep -Fq 'fall: landed' "$LOG_DIR/engine.log" && fail 'a fall was charged for a jump or a teleport drop'
# Acrobatics gained only the jump's 0.3 (a fall would add 3.0).
python3 - "$(banked "$LOG_DIR/before.log" Acrobatics)" "$(skill_value "$LOG_DIR/before.log" Acrobatics)" "$(banked "$LOG_DIR/after.log" Acrobatics)" "$(skill_value "$LOG_DIR/after.log" Acrobatics)" <<'PY' || fail 'Acrobatics gained more than a jump is worth'
import sys
b0, v0, b1, v1 = float(sys.argv[1] or 0), int(sys.argv[2] or 5), float(sys.argv[3] or 0), int(sys.argv[4] or 5)
assert v1 == v0 and 0 <= b1 - b0 < 1.0, (b0, v0, b1, v1)
PY
printf 'smoke[%s]: PASS -- a jump and a 300-unit teleport drop cost no Health (%s) and no fall experience; artifacts: %s\n' "$GATE" "$hp_after" "$LOG_DIR"
