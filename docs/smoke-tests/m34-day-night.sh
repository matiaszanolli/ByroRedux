#!/usr/bin/env bash
# M34 day/night runtime smoke — drive the persistent clock through byro-dbg
# against a real Skyrim exterior and verify the climate-driven sun responds at
# sunrise, noon, and night.

set -euo pipefail

SKYRIM_DATA="${BYROREDUX_SKYRIM_DATA:-/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data}"
PORT="${BYRO_DEBUG_PORT:-9876}"
BENCH_FRAMES="${BYROREDUX_SMOKE_FRAMES:-30}"
GRID="${BYROREDUX_DAY_NIGHT_GRID:-2,-4}"
ENTITY_FLOOR="${BYROREDUX_ENTITY_FLOOR:-1000}"

LOG_DIR="$(mktemp -d)"
engine_pid=""
cleanup() {
    if [[ -n "$engine_pid" ]]; then
        kill -TERM "$engine_pid" 2>/dev/null || true
        wait "$engine_pid" 2>/dev/null || true
    fi
    rm -rf "$LOG_DIR"
}
trap cleanup EXIT

required=(
    "$SKYRIM_DATA/Skyrim.esm"
    "$SKYRIM_DATA/Skyrim - Meshes0.bsa"
    "$SKYRIM_DATA/Skyrim - Meshes1.bsa"
    "$SKYRIM_DATA/Skyrim - Textures0.bsa"
)
for path in "${required[@]}"; do
    if [[ ! -f "$path" ]]; then
        echo "smoke[m34-day-night]: SKIP — required data not found: $path"
        exit 0
    fi
done

echo "═══════════════════════════════════════════════════════════════"
echo "  smoke[m34-day-night]: Skyrim Tamriel grid $GRID"
echo "═══════════════════════════════════════════════════════════════"

engine_stdout="$LOG_DIR/engine.stdout"
engine_stderr="$LOG_DIR/engine.stderr"
debug_log="$LOG_DIR/debug.log"

cargo run --release --quiet -- \
    --esm "$SKYRIM_DATA/Skyrim.esm" \
    --grid "$GRID" \
    --radius 1 \
    --wrld Tamriel \
    --bsa "$SKYRIM_DATA/Skyrim - Meshes0.bsa" \
    --bsa "$SKYRIM_DATA/Skyrim - Meshes1.bsa" \
    --textures-bsa "$SKYRIM_DATA/Skyrim - Textures0.bsa" \
    --bench-frames "$BENCH_FRAMES" \
    --bench-hold \
    >"$engine_stdout" 2>"$engine_stderr" &
engine_pid=$!

deadline=$(( $(date +%s) + 180 ))
while ! grep -q '^bench-hold:' "$engine_stderr" 2>/dev/null; do
    if [[ $(date +%s) -gt $deadline ]]; then
        echo "smoke[m34-day-night]: FAIL — timeout waiting for bench-hold"
        tail -20 "$engine_stderr" || true
        exit 1
    fi
    if ! kill -0 "$engine_pid" 2>/dev/null; then
        echo "smoke[m34-day-night]: FAIL — engine exited before bench-hold"
        tail -20 "$engine_stderr" || true
        exit 1
    fi
    sleep 0.5
done

BYRO_DEBUG_PORT="$PORT" cargo run --release --quiet -p byro-dbg <<EOF >"$debug_log" 2>&1
time.pause
time.set 06:00
time.show
time.set 12:00
time.show
time.set 23:00
time.show
time.scale 120
time.pause
time.resume
time.pause
time.advance 25
time.show
quit
EOF

bench_line="$(grep '^bench:' "$engine_stdout" | tail -1 || true)"
if [[ -z "$bench_line" ]]; then
    echo "smoke[m34-day-night]: FAIL — no bench summary"
    exit 1
fi
echo "$bench_line"

entities="$(echo "$bench_line" | grep -oE 'entities=[0-9]+' | head -1 | cut -d= -f2)"
: "${entities:=0}"
if (( entities < ENTITY_FLOOR )); then
    echo "smoke[m34-day-night]: FAIL — entities=$entities < floor $ENTITY_FLOOR"
    exit 1
fi

hard_fail=0
require_output() {
    local pattern="$1"
    local description="$2"
    if grep -Fq "$pattern" "$debug_log"; then
        echo "smoke[m34-day-night]: PASS — $description"
    else
        echo "smoke[m34-day-night]: FAIL — $description (missing '$pattern')"
        hard_fail=1
    fi
}

require_output "time.set: day=0 hour=6.000 clock=06:00 phase=sunrise" "sunrise phase is selectable"
require_output "time.set: day=0 hour=12.000 clock=12:00 phase=day" "day phase is selectable"
require_output "sun: intensity=4.000" "noon produces full exterior sun"
require_output "time.set: day=0 hour=23.000 clock=23:00 phase=night" "night phase is selectable"
require_output "sun: intensity=0.000" "night removes exterior sun intensity"
require_output "time.resume: day=0 hour=23.000 clock=23:00 phase=night scale=120.000x paused=false" "pause/resume retains the configured rate"
require_output "time.advance: day=2 hour=0.000 clock=00:00 phase=night" "multi-day advance carries day count and wraps the clock"

if grep -Fq 'Error:' "$debug_log"; then
    echo "smoke[m34-day-night]: FAIL — command output contained an error"
    hard_fail=1
fi

if (( hard_fail != 0 )); then
    echo "── byro-dbg output ─────────────────────────────────────────────"
    cat "$debug_log"
    exit "$hard_fail"
fi

echo "smoke[m34-day-night]: PASS — persistent clock drives the real exterior sun cycle"
