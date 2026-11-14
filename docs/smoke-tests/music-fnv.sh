#!/usr/bin/env bash
# Game Music, PRD §7.2 scenario 6 (#6886) and scenario 5's New Vegas leg (#6881): media sets on New
# Vegas, headless.
#
#   6. At Sloan (WastelandNV grid (-8,1)), inside audio marker 0x0015D028 (musCtrlSloan, radius 7000),
#      music.status reports the controller's Location set musSetLOCDesSttlBothABC with its day layers
#      at noon and its night layers at 02:00.
#   6b. At Goodsprings, marker 0x00152433's controller (musCtrlGoodsprings) carries faction-relation
#      lists only: the Neutral set plays while GoodspringsFaction (0x00104C6E) is neutral to the
#      player, and its Enemy set once SetFactionReaction (0x10a8) turns it hostile to PlayerFaction
#      (0x0001B2A4) (#6904).
#   5. The intro's own `PlayBink "FNVIntro.bik" 1 1 0 1` (pause music off) raises no Hold layer.
#
#   docs/smoke-tests/music-fnv.sh

set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/fixture.sh"
smoke_load_fixture music-fnv fnv
smoke_require_data
[[ -d "$SMOKE_DATA/Music" ]] || { echo "smoke[music-fnv]: SKIP -- no Data/Music"; exit 77; }
smoke_require_fixture_fields P0_CELL

ROOT_DIR="$SMOKE_ROOT_DIR"
ENGINE_BIN="$ROOT_DIR/target/release/byroredux"
DEBUG_BIN="$ROOT_DIR/target/release/byro-dbg"
PORT="${BYRO_DEBUG_PORT:-9876}"

LOG_DIR="$(mktemp -d /tmp/byro-music-fnv.XXXXXX)"
engine_pid=""
keep_artifacts="${BYROREDUX_KEEP_ARTIFACTS:-0}"
stop_engine() {
    if [[ -n "$engine_pid" ]]; then
        kill -TERM "$engine_pid" 2>/dev/null || true
        wait "$engine_pid" 2>/dev/null || true
        engine_pid=""
    fi
    local orphan
    orphan=$(ss -tlnp 2>/dev/null | grep ":$PORT " | grep -oP 'pid=\K[0-9]+' | head -1 || true)
    [[ -z "$orphan" ]] || kill "$orphan" 2>/dev/null || true
    for _ in $(seq 1 20); do ss -tln 2>/dev/null | grep -q ":$PORT " || break; sleep 0.5; done
}
cleanup() {
    stop_engine
    (( keep_artifacts == 0 )) && rm -rf "$LOG_DIR"
    return 0
}
trap cleanup EXIT INT TERM

fail() {
    keep_artifacts=1
    echo "smoke[music-fnv]: FAIL -- $*"
    echo "smoke[music-fnv]: artifacts retained at $LOG_DIR"
    grep 'music' "$LOG_DIR"/engine.*.stderr 2>/dev/null | tail -10 || true
    exit 1
}
pass() { echo "smoke[music-fnv]: PASS -- $*"; }

if [[ ! -x "$ENGINE_BIN" || ! -x "$DEBUG_BIN" ]]; then
    echo "smoke[music-fnv]: building release binaries"
    (cd "$ROOT_DIR" && cargo build --release --quiet -p byroredux -p byro-dbg)
fi

ask() {
    { printf '%s\n' "$@"; echo .quit; } | env BYRO_DEBUG_PORT="$PORT" "$DEBUG_BIN" >"$LOG_DIR/ask.tmp" 2>&1
    sed 's/\\n/\n/g; s/\\"/"/g; s/\\\\/\\/g' "$LOG_DIR/ask.tmp"
}
layer() { ask music.status | grep -o "^$1: .*" | head -1; }
position() { grep -oE 'at [0-9]+\.[0-9]s' <<<"$1" | head -1 | grep -oE '[0-9]+\.[0-9]'; }
wait_layer() {
    local layer="$1" seconds="$2" description="$3" pattern="$4" deadline=$((SECONDS + $2))
    until [[ "$(layer "$layer")" =~ $pattern ]]; do
        kill -0 "$engine_pid" 2>/dev/null || fail "the engine exited waiting for: $description"
        (( SECONDS < deadline )) || fail "timeout (${seconds}s) waiting for: $description ($(layer "$layer"))"
        sleep 1
    done
}

# start_engine <label> <engine args...>: launch headless and wait for the bench hold.
start_engine() {
    local label="$1"
    shift
    stop_engine
    (
        cd "$SMOKE_DATA"
        exec env BYRO_DEBUG_PORT="$PORT" BYRO_DEBUG_SERVER=1 \
            BYROREDUX_SETTINGS_PATH="$LOG_DIR/settings.toml" \
            BYROREDUX_SAVE_DIR="$LOG_DIR/saves" \
            RUST_LOG="warn,byroredux::music=info" \
            "${SMOKE_HEADLESS[@]}" "$ENGINE_BIN" "${SMOKE_ENGINE_ARGS[@]}" "$@" --bench-frames 30 --bench-hold
    ) >"$LOG_DIR/engine.$label.stdout" 2>"$LOG_DIR/engine.$label.stderr" &
    engine_pid=$!
    local deadline=$((SECONDS + 300))
    until grep -q 'bench-hold:' "$LOG_DIR/engine.$label.stderr" 2>/dev/null; do
        kill -0 "$engine_pid" 2>/dev/null || fail "engine exited before bench-hold ($label)"
        (( SECONDS < deadline )) || fail "timeout waiting for bench-hold ($label)"
        sleep 0.5
    done
}

# ── 6 — an audio marker's Location set, by day and by night ─────────────────
start_engine sloan --wrld WastelandNV --grid -8,1 --radius 1
ask "time.set 12" >/dev/null
wait_layer Location 20 "Sloan's media set by day" '^Location: Playing — musSetLOCDesSttlBothABC .*audio marker.*_day_.*controller musCtrlSloan'
day=$(layer Location)
[[ "$day" != *"_night_"* ]] || fail "night layers play at noon ($day)"
ask music.status | grep -q 'audio marker 0x0015d028' || fail "the marker is not reported"
pass "noon: $day"
ask "time.set 2" >/dev/null
wait_layer Location 20 "Sloan's media set by night" '^Location: Playing — musSetLOCDesSttlBothABC .*_night_'
night=$(layer Location)
[[ "$night" != *"_day_"* ]] || fail "day layers play at 02:00 ($night)"
pass "02:00: $night"

start_engine goodsprings --wrld WastelandNV --grid -18,0 --radius 1
wait_layer Location 20 "Goodsprings' Neutral set" '^Location: Playing — musSetLOCDesExplBothABC .*audio marker Neutral set.*Neutral to the player'
pass "Goodsprings, neutral: $(layer Location)"
ask "obscript.cmd none 10a8 r:00104C6E r:0001B2A4 -100" >/dev/null
wait_layer Location 10 "Goodsprings' Enemy set" '^Location: .*audio marker Enemy set.*Enemy to the player'
pass "Goodsprings, hostile: $(layer Location)"

# ── 5 — the intro's PlayBink leaves the music playing ────────────────────────
if [[ -f "$SMOKE_DATA/Video/FNVIntro.bik" ]]; then
    start_engine intro --cell "$P0_CELL"
    ask "obscript.cmd none 1114 t:FNVIntro.bik 1 1 0 1" >/dev/null
    deadline=$((SECONDS + 30))
    until ask video.status | grep -q 'playing FNVIntro.bik frame [1-9]'; do
        (( SECONDS < deadline )) || fail "FNVIntro.bik did not start"
        sleep 1
    done
    ask music.status | grep -q "Hold: off" || fail "the intro's pause-music-off PlayBink raised the Hold layer"
    pass "FNVIntro.bik 1 1 0 1 raises no Hold layer"
fi

echo "smoke[music-fnv]: PASS -- New Vegas media sets play by area, day and night"
