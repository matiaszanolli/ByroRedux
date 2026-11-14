#!/usr/bin/env bash
# Game Music, PRD §7.2 scenario 7 (#6890): Oblivion's music categories and StreamMusic, headless.
#
#   7. BrumaJGhastasHouse (Public) and SageGlenHollow (Dungeon) report Location from public\ and
#      dungeon\; `StreamMusic Random` (0x104e) reports the Script layer playing one of the three
#      folders, and a streamed file that does not exist reports Missing and leaves it playing.
#      Death plays the Special\death.mp3 sting.
#
#   docs/smoke-tests/music-oblivion.sh

set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/fixture.sh"
smoke_load_fixture music-oblivion oblivion
smoke_require_data
[[ -d "$SMOKE_DATA/Music" ]] || { echo "smoke[music-oblivion]: SKIP -- no Data/Music"; exit 77; }

ROOT_DIR="$SMOKE_ROOT_DIR"
ENGINE_BIN="$ROOT_DIR/target/release/byroredux"
DEBUG_BIN="$ROOT_DIR/target/release/byro-dbg"
PORT="${BYRO_DEBUG_PORT:-9876}"

LOG_DIR="$(mktemp -d /tmp/byro-music-oblivion.XXXXXX)"
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
    echo "smoke[music-oblivion]: FAIL -- $*"
    echo "smoke[music-oblivion]: artifacts retained at $LOG_DIR"
    grep 'music' "$LOG_DIR"/engine.*.stderr 2>/dev/null | tail -10 || true
    exit 1
}
pass() { echo "smoke[music-oblivion]: PASS -- $*"; }

if [[ ! -x "$ENGINE_BIN" || ! -x "$DEBUG_BIN" ]]; then
    echo "smoke[music-oblivion]: building release binaries"
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

# ── 7 — categories ───────────────────────────────────────────────────────────
start_engine dungeon --cell SageGlenHollow
wait_layer Location 20 "SageGlenHollow's music" '^Location: Playing — Dungeon \(cell XCMT\) — music\\dungeon\\'
pass "SageGlenHollow: $(layer Location)"

start_engine public --cell BrumaJGhastasHouse
wait_layer Location 20 "BrumaJGhastasHouse's music" '^Location: Playing — Public \(cell XCMT\) — music\\public\\'
pass "BrumaJGhastasHouse: $(layer Location)"

# ── StreamMusic ──────────────────────────────────────────────────────────────
ask "obscript.cmd none 104e t:Random" >/dev/null
wait_layer Script 10 "StreamMusic Random" '^Script: Playing — Random \(StreamMusic\) — music\\(public|explore|dungeon)\\'
streamed=$(layer Script)
pass "StreamMusic Random: $streamed"
ask 'obscript.cmd none 104e t:"Data\Music\Special\nope.mp3"' >/dev/null
sleep 1
[[ "$(layer Script)" == *"Random (StreamMusic)"* ]] || fail "a missing StreamMusic file replaced the playing track ($(layer Script))"
pass "a missing StreamMusic file leaves the streamed track playing"

# ── stings ───────────────────────────────────────────────────────────────────
# The level-up sting is translated (Special\success.mp3, a unit test pins it) but has no driver here:
# Oblivion levels by major-skill increases, which the engine does not model yet (`levelup xp` awards
# no level under classic-TES rules).
ask "obscript.cmd 0x14 108b" >/dev/null
wait_layer Sting 10 "the death sting" '^Sting: Playing — death .* music\\special\\death\.mp3'
pass "the player's death: $(layer Sting)"

echo "smoke[music-oblivion]: PASS -- Oblivion plays its categories, StreamMusic and stings"
