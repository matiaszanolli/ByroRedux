#!/usr/bin/env bash
# Owner-run live gate for OAV §7.2 scenario 4. Requires a Vulkan device,
# display, and BYROREDUX_OBLIVION_DATA. Artifacts remain in the printed folder.
set -euo pipefail

DATA="${BYROREDUX_OBLIVION_DATA:-}"
if [[ -z "$DATA" || ! -f "$DATA/Oblivion.esm" || ! -f "$DATA/Oblivion - Meshes.bsa" || ! -f "$DATA/Oblivion - Textures - Compressed.bsa" ]]; then
    echo 'smoke[oblivion-actor-values]: SKIP -- missing BYROREDUX_OBLIVION_DATA (set it to the Oblivion Data directory)'
    exit 77
fi
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
DATA="$(cd "$DATA" && pwd)"
cd "$ROOT"
PORT="${BYRO_DEBUG_PORT:-19881}"
TIMEOUT="${BYROREDUX_SMOKE_TIMEOUT:-300}"
LOG_DIR="$(mktemp -d /tmp/byro-oblivion-actor-values.XXXXXX)"
engine_pid=""
stop_engine() {
    if [[ -n "$engine_pid" ]]; then
        kill -TERM "$engine_pid" 2>/dev/null || true
        wait "$engine_pid" 2>/dev/null || true
    fi
}
trap stop_engine EXIT
fail() { echo "smoke[oblivion-actor-values]: FAIL -- $*; artifacts: $LOG_DIR" >&2; exit 1; }
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
    --player --hud --bench-frames 30 --bench-hold >"$LOG_DIR/engine.log" 2>&1 &
engine_pid=$!
deadline=$((SECONDS + TIMEOUT))
until grep -Fq 'bench-hold:' "$LOG_DIR/engine.log"; do
    kill -0 "$engine_pid" 2>/dev/null || fail 'engine exited before bench-hold'
    (( SECONDS < deadline )) || fail 'timed out waiting for bench-hold'
    sleep 0.25
done
command 'stealth.sneak on' "$LOG_DIR/sneak.log" || fail 'sneak command failed'
sleep 2
command 'stealth.status' "$LOG_DIR/stealth.log" || fail 'stealth.status failed'
grep -Fq 'rules Oblivion' "$LOG_DIR/stealth.log" || fail 'no Oblivion detection rules'
entity="$(sed -nE 's/.*entity ([0-9]+) ref .*/\1/p' "$LOG_DIR/stealth.log" | head -1)"
[[ -n "$entity" ]] || fail 'no detector in the cell'
command "obscript.cmd $entity GetAV Sneak" "$LOG_DIR/getav.log" || fail 'GetAV failed'
grep -Eq -- '0x100e -> [0-9]+(\.[0-9]+)?' "$LOG_DIR/getav.log" || fail 'GetAV returned no numeric Sneak value'
python3 - "$LOG_DIR/stealth.log" <<'PY' || fail 'no non-zero detection skill term'
import re, sys
text = open(sys.argv[1], encoding="utf-8").read()
assert any(float(x) != 0 for x in re.findall(r"\bskill (-?\d+(?:\.\d+)?)", text))
PY
command 'hud.status' "$LOG_DIR/hud-before.log" || fail 'hud.status failed'
command 'obscript.cmd 0x14 DamageAV Health 10' "$LOG_DIR/damage.log" || fail 'DamageAV failed'
grep -q '\[declined:' "$LOG_DIR/damage.log" && fail 'DamageAV was declined'
sleep 1
command 'hud.status' "$LOG_DIR/hud-after.log" || fail 'hud.status failed after damage'
python3 - "$LOG_DIR/hud-before.log" "$LOG_DIR/hud-after.log" <<'PY' || fail 'HUD health did not fall after damage'
import re, sys
values = []
for path in sys.argv[1:]:
    text = open(path, encoding="utf-8").read()
    match = re.search(r"(?i)\bhealth=(\d+(?:\.\d+)?) \(auto\)", text)
    assert match, text
    values.append(float(match.group(1)))
assert values[1] < values[0], values
PY
printf 'smoke[oblivion-actor-values]: PASS -- detector %s; artifacts: %s\n' "$entity" "$LOG_DIR"
