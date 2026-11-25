#!/usr/bin/env bash
# xWMA scenario 4 and combat ducking. Run with skyrim_se or fo4 and a working audio device.
set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/fixture.sh"
smoke_load_fixture xwma-music "$@"
smoke_require_data
command -v python3 >/dev/null || { echo "smoke[xwma-music]: SKIP -- missing python3"; exit 77; }

case "$SMOKE_GAME" in
    skyrim_se) worldspace=Tamriel grid=7,3 actor=0x000DD8FF combat_type=MUSCombat ;;
    fo4) worldspace=Commonwealth grid=-19,16 actor=0x0006D3A3 combat_type=MUSzCombat ;;
    *) exit 2 ;;
esac

engine="${BYRO_BIN:-$SMOKE_ROOT_DIR/target/release/byroredux}"
client="${BYRO_DBG_BIN:-$SMOKE_ROOT_DIR/target/release/byro-dbg}"
port="${BYRO_DEBUG_PORT:-19976}"
timeout_seconds="${BYROREDUX_SMOKE_TIMEOUT:-180}"
logs="$(mktemp -d /tmp/byro-xwma-music-gate.XXXXXX)"
engine_pid=
cleanup() {
    if [[ -n "$engine_pid" ]]; then
        kill -TERM "$engine_pid" 2>/dev/null || true
        wait "$engine_pid" 2>/dev/null || true
    fi
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

fail() { echo "smoke[xwma-music]: FAIL -- $* (artifacts: $logs)" >&2; exit 1; }
if ss -ltn "sport = :$port" | grep -q LISTEN; then
    fail "port $port is already occupied"
fi
if [[ ! -x "$engine" || ! -x "$client" ]]; then
    (cd "$SMOKE_ROOT_DIR" && cargo build --release --offline --quiet -p byroredux -p byro-dbg)
fi

debug() {
    printf '%s\n' "$@" .quit | BYRO_DEBUG_PORT="$port" timeout 5s "$client" >>"$logs/console.log" 2>&1
}
read_music() {
    printf '%s\n' music.status .quit | BYRO_DEBUG_PORT="$port" timeout 5s "$client" 2>>"$logs/console.log" |
        python3 -c '
import json
import sys

for line in sys.stdin:
    reply = line.removeprefix("byro> ").strip()
    if reply.startswith("\""):
        print(json.loads(reply))
' | tee -a "$logs/console.log"
}
wait_music() {
    local layer="$1" pattern="$2" deadline=$((SECONDS + timeout_seconds)) result
    while (( SECONDS < deadline )); do
        kill -0 "$engine_pid" 2>/dev/null || fail "engine exited while waiting for $layer"
        if result="$(read_music)"; then
            [[ "$result" != *"no audio device"* ]] || fail "a working audio device is required"
            if grep -Eq "$pattern" <<<"$result"; then
                grep -m1 "^$layer: " <<<"$result"
                return
            fi
        fi
        sleep 0.5
    done
    fail "timed out waiting for $layer to match $pattern"
}
position() {
    case "$1" in
        *' — palette '*) sed -nE 's/.* — palette .+ ([[:digit:].]+)\/[[:digit:].]+s.*/\1/p' <<<"$1" ;;
        *) sed -nE 's/.*\.xwm at ([[:digit:].]+)s.*/\1/p' <<<"$1" ;;
    esac
}
selection() {
    case "$1" in
        *' — palette '*) sed -nE 's/.* — palette (.+) [[:digit:].]+\/[[:digit:].]+s.*/\1/p' <<<"$1" ;;
        *) sed -nE 's/.* — (.+\.xwm) at .*/\1/p' <<<"$1" ;;
    esac
}

(
    cd "$SMOKE_DATA"
    exec env BYRO_DEBUG_PORT="$port" BYRO_DEBUG_SERVER=1 \
        BYROREDUX_SETTINGS_PATH="$logs/settings.toml" BYROREDUX_SAVE_DIR="$logs/saves" \
        RUST_LOG=info "${SMOKE_HEADLESS[@]}" "$engine" "${SMOKE_ENGINE_ARGS[@]}" \
        --wrld "$worldspace" --grid "$grid" --radius 1 --player \
        --window-size 640x360 --bench-frames 5 --bench-hold
) >"$logs/engine.log" 2>&1 &
engine_pid=$!

deadline=$((SECONDS + timeout_seconds))
until grep -Fq 'bench-hold:' "$logs/engine.log"; do
    kill -0 "$engine_pid" 2>/dev/null || fail "engine exited during startup"
    (( SECONDS < deadline )) || fail "engine startup timed out"
    sleep 0.5
done

location='^Location: Playing — .*\.xwm at '
first="$(wait_music Location "$location")"
sleep 2
second="$(wait_music Location "$location")"
awk -v a="$(position "$first")" -v b="$(position "$second")" 'BEGIN { exit !(b > a) }' \
    || fail "exterior position did not advance ($first → $second)"
echo "smoke[xwma-music]: $SMOKE_GAME exterior PASS -- $second"

debug "obscript.cmd $actor 1016 r:00000014" || fail "could not request combat"
battle="$(wait_music Battle "^Battle: Playing — $combat_type .*\.xwm at ")"
ducked='^Location: (Playing \(ducked under Battle by 100\.0 dB\)|Silent \(timed selection, ducked under Battle by 100\.0 dB\))'
quiet="$(wait_music Location "$ducked")"
sleep 2
later="$(wait_music Battle "^Battle: Playing — $combat_type .*\.xwm at ")"
quieter="$(wait_music Location "$ducked")"
awk -v a="$(position "$battle")" -v b="$(position "$later")" 'BEGIN { exit !(b > a) }' \
    || fail "battle position did not advance ($battle → $later)"
awk -v a="$(position "$quiet")" -v b="$(position "$quieter")" 'BEGIN { exit !(b > a) }' \
    || fail "ducked exterior position did not advance ($quiet → $quieter)"
[[ "$(selection "$second")" == "$(selection "$quieter")" ]] || fail "ducking switched the exterior selection"
echo "smoke[xwma-music]: $SMOKE_GAME combat PASS -- $later / $quieter"

debug "obscript.cmd $actor 1017 r:00000014" || fail "could not end combat"
wait_music Battle '^Battle: none' >/dev/null
restored="$(wait_music Location '^(Location: Playing — .*\.xwm at |Location: Silent \(timed selection\) — .* — palette )')"
[[ "$(selection "$quieter")" == "$(selection "$restored")" ]] || fail "restoring gain switched the exterior selection"
awk -v a="$(position "$quieter")" -v b="$(position "$restored")" 'BEGIN { exit !(b > a) }' \
    || fail "exterior position did not remain continuous ($quieter → $restored)"
echo "smoke[xwma-music]: $SMOKE_GAME exterior gain restored -- $restored"
debug engine.quit || fail "could not shut down the engine"
wait "$engine_pid" || fail "engine exited unsuccessfully"
engine_pid=
echo "artifacts: $logs"
