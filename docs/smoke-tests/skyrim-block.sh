#!/usr/bin/env bash
# Four fresh Skyrim SE sessions. Mikael lands an ordinary dagger strike while
# the player holds Block at skill 100. Use xvfb-run -a on a display-less host.
set -euo pipefail

root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"
data="${BYROREDUX_SKYRIMSE_DATA:-/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data}"
port="${BYRO_DEBUG_PORT:-19972}"
timeout_seconds="${BYROREDUX_SMOKE_TIMEOUT:-120}"
engine_bin="${BYRO_BIN:-$root/target/release/byroredux}"
debug_bin="${BYRO_DBG_BIN:-$root/target/release/byro-dbg}"
engine_pid=""

for archive in 'Skyrim.esm' 'Skyrim - Meshes0.bsa' 'Skyrim - Meshes1.bsa' \
               'Skyrim - Textures0.bsa' 'Skyrim - Misc.bsa'; do
    if [[ ! -f "$data/$archive" ]]; then
        echo "smoke[skyrim-block]: SKIP -- missing $data/$archive"
        exit 77
    fi
done

logs="$(mktemp -d /tmp/byro-skyrim-block-gate.XXXXXX)"
fail() { echo "smoke[skyrim-block]: FAIL -- $*; artifacts: $logs" >&2; exit 1; }
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

if ss -ltn "sport = :$port" | grep -q LISTEN; then
    fail "port $port is already occupied"
fi
if [[ -z "${BYRO_BIN:-}" || -z "${BYRO_DBG_BIN:-}" ]]; then
    cargo build --release --offline --quiet -p byroredux -p byro-dbg || fail 'release build failed'
fi
[[ -x "$engine_bin" && -x "$debug_bin" ]] || fail 'missing engine or debug client binary'

debug() {
    printf '%s\n' "$@" '.quit' | BYRO_DEBUG_PORT="$port" timeout "$timeout_seconds" "$debug_bin" 2>>"$console"
}

run_trial() {
    local trial="$1" item="$2" hands="$3" expected="$4"
    local console="$logs/$trial-console.log" log="$logs/$trial-engine.log"
    local status player attacker armor deadline
    env BYRO_DEBUG_PORT="$port" BYRO_DEBUG_SERVER=1 BYROREDUX_SETTINGS_PATH="$logs/$trial-settings.toml" \
        "$engine_bin" --esm "$data/Skyrim.esm" --cell WhiterunBanneredMare \
        --bsa "$data/Skyrim - Meshes0.bsa" --bsa "$data/Skyrim - Meshes1.bsa" \
        --textures-bsa "$data/Skyrim - Textures0.bsa" --scripts-bsa "$data/Skyrim - Misc.bsa" \
        --player --bench-frames 5 --bench-hold >"$log" 2>&1 &
    engine_pid=$!
    deadline=$((SECONDS + timeout_seconds))
    until grep -Fq 'bench-hold:' "$log"; do
        kill -0 "$engine_pid" 2>/dev/null || fail "$trial engine quit during startup"
        (( SECONDS < deadline )) || fail "$trial startup timed out"
        sleep 0.25
    done

    status="$(debug 'player.status')"
    printf '%s\n' "$status" >>"$console"
    player="$(sed -n 's/^.*mode=Character player=\([0-9]*\).*$/\1/p' <<<"$status")"
    [[ "$player" =~ ^[0-9]+$ ]] || fail "$trial has no character player"
    debug "setav $player 44F 100" >>"$console"
    # Make the torch and bare-hands legs independent of the starting axe.
    if [[ "$trial" == torch || "$trial" == hands ]]; then
        debug 'inv.equip 13790' >>"$console"
    fi
    if [[ -n "$item" ]]; then
        debug "inv.add $item" "inv.equip $item" >>"$console"
    fi
    deadline=$((SECONDS + timeout_seconds))
    until status="$(debug 'inventory.status')" && grep -Fq "$hands" <<<"$status"; do
        (( SECONDS < deadline )) || fail "$trial did not equip the expected guard"
        sleep 0.25
    done
    printf '%s\n' "$status" >>"$console"
    armor="$(debug 'armor.status')"
    printf '%s\n' "$armor" >>"$console"

    status="$(debug 'combat.approach 0x0001A671')"
    printf '%s\n' "$status" >>"$console"
    attacker="$(sed -n 's/^.*placed in melee range of entity \([0-9]*\).*$/\1/p' <<<"$status")"
    [[ "$attacker" =~ ^[0-9]+$ ]] || fail "$trial could not face Mikael"
    debug 'input.hold block 10000' "obscript.cmd $attacker 0x1016 r:0x14" >>"$console"
    deadline=$((SECONDS + timeout_seconds))
    until status="$(debug 'combat.status')" && grep -Fq "last_target=$player incoming=4.0" <<<"$status"; do
        (( SECONDS < deadline )) || fail "$trial never took Mikael's dagger hit"
        sleep 0.25
    done
    printf '%s\n' "$status" >>"$console"
    python3 - "$expected" "$armor" "$status" <<'PY' || fail "$trial damage differed from the expected guard"
import re
import sys

block, armor, hit = sys.argv[1:]
expected = float(block)
resisted = float(re.search(r"physical damage reduction ([\d.]+)%", armor)[1]) / 100
incoming, damage = map(float, re.search(r"incoming=([\d.]+) damage=([\d.]+)", hit).groups())
reported = re.search(r"blocked (\d+)%", hit)
assert "blocking=true" in hit, hit
assert (int(reported[1]) if reported else 0) == round(expected * 100), hit
assert abs(damage - incoming * (1 - resisted) * (1 - expected)) <= 0.051, hit
PY

    debug 'engine.quit' >>"$console"
    deadline=$((SECONDS + timeout_seconds))
    while kill -0 "$engine_pid" 2>/dev/null; do
        (( SECONDS < deadline )) || fail "$trial did not shut down"
        sleep 0.25
    done
    wait "$engine_pid" || fail "$trial exited unsuccessfully"
    engine_pid=""
    echo "smoke[skyrim-block]: $trial PASS"
}

run_trial shield 12EB6 'hands left=10 00012EB6 right=3 00013790' 0.55
run_trial sword 12EB7 'hands left=0 right=1 00012EB7' 0.32
run_trial torch 1D4EC 'hands left=11 0001D4EC right=0' 0.32
run_trial hands '' 'hands left=0 right=0' 0
echo "artifacts: $logs"
