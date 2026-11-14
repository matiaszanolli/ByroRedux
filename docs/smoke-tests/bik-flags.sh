#!/usr/bin/env bash
# Bink video playback, PRD scenario 3 (BIK-PRES-004 / BIK-PRES-005 / BIK-RPT-003, #6858): a
# PlayBink request keeps its flags. FNV, headless:
#
#   1. `obscript.cmd none 1114 t:FNVIntro.bik 1 1 0 1` (VCG00's flags): video.status reports the
#      movie with interruptable=true, the world muted and the music left playing; video.skip ends it
#      as Played (ended early).
#   2. `obscript.cmd none 1114 t:FNVIntro.bik 0`: video.skip does not end it; it keeps playing.
#
#   docs/smoke-tests/bik-flags.sh

set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/fixture.sh"
smoke_load_fixture bik-flags fnv
smoke_require_fixture_fields P0_CELL
smoke_require_data
[[ -f "$SMOKE_DATA/Video/FNVIntro.bik" ]] || { echo "smoke[bik-flags]: SKIP -- no Video/FNVIntro.bik"; exit 77; }

ROOT_DIR="$SMOKE_ROOT_DIR"
ENGINE_BIN="$ROOT_DIR/target/release/byroredux"
DEBUG_BIN="$ROOT_DIR/target/release/byro-dbg"
PORT="${BYRO_DEBUG_PORT:-9876}"

LOG_DIR="$(mktemp -d /tmp/byro-bik-flags.XXXXXX)"
engine_pid=""
keep_artifacts="${BYROREDUX_KEEP_ARTIFACTS:-0}"
cleanup() {
    if [[ -n "$engine_pid" ]]; then
        kill -TERM "$engine_pid" 2>/dev/null || true
        wait "$engine_pid" 2>/dev/null || true
    fi
    local orphan
    orphan=$(ss -tlnp 2>/dev/null | grep ":$PORT " | grep -oP 'pid=\K[0-9]+' | head -1 || true)
    [[ -z "$orphan" ]] || kill "$orphan" 2>/dev/null || true
    (( keep_artifacts == 0 )) && rm -rf "$LOG_DIR"
    return 0
}
trap cleanup EXIT INT TERM

fail() {
    keep_artifacts=1
    echo "smoke[bik-flags]: FAIL -- $*"
    echo "smoke[bik-flags]: artifacts retained at $LOG_DIR"
    grep 'video:' "$LOG_DIR/engine.stderr" 2>/dev/null | tail -10 || true
    exit 1
}
pass() { echo "smoke[bik-flags]: PASS -- $*"; }

if [[ ! -x "$ENGINE_BIN" || ! -x "$DEBUG_BIN" ]]; then
    echo "smoke[bik-flags]: building release binaries"
    (cd "$ROOT_DIR" && cargo build --release --quiet -p byroredux -p byro-dbg)
fi

ask() {
    { printf '%s\n' "$1"; echo .quit; } | env BYRO_DEBUG_PORT="$PORT" "$DEBUG_BIN" >"$LOG_DIR/ask.tmp" 2>&1
    sed 's/\\n/\n/g; s/\\"/"/g' "$LOG_DIR/ask.tmp"
}
status() { ask video.status | grep -o 'video: .*' | head -1; }
wait_status() {
    local seconds="$1" description="$2" pattern="$3" deadline=$((SECONDS + $1))
    until [[ "$(status)" =~ $pattern ]]; do
        kill -0 "$engine_pid" 2>/dev/null || fail "the engine exited waiting for: $description"
        (( SECONDS < deadline )) || fail "timeout (${seconds}s) waiting for: $description ($(status))"
        sleep 1
    done
}

cd "$SMOKE_DATA"
env BYRO_DEBUG_PORT="$PORT" BYRO_DEBUG_SERVER=1 \
    BYROREDUX_SETTINGS_PATH="$LOG_DIR/settings.toml" \
    BYROREDUX_SAVE_DIR="$LOG_DIR/saves" \
    RUST_LOG="warn,byroredux::video=info" \
    "${SMOKE_HEADLESS[@]}" "$ENGINE_BIN" "${SMOKE_ENGINE_ARGS[@]}" \
    --cell "$P0_CELL" --bench-frames 30 --bench-hold \
    >"$LOG_DIR/engine.stdout" 2>"$LOG_DIR/engine.stderr" &
engine_pid=$!

deadline=$((SECONDS + 300))
until grep -q 'bench-hold:' "$LOG_DIR/engine.stderr" 2>/dev/null; do
    kill -0 "$engine_pid" 2>/dev/null || fail "engine exited before bench-hold"
    (( SECONDS < deadline )) || fail "timeout waiting for bench-hold"
    sleep 0.5
done
pass "engine held in $P0_CELL"

# ── 1 — interruptable, muted, music left playing ─────────────────────────────
ask "obscript.cmd none 1114 t:FNVIntro.bik 1 1 0 1" >/dev/null
wait_status 30 "FNVIntro.bik playing" "playing FNVIntro.bik frame [1-9][0-9]*/"
s=$(status)
[[ "$s" == *"interruptable=true mute=true pause_music=false letterbox=true"* ]] || fail "the flags did not reach the player ($s)"
[[ "$s" == *"holding world_muted=true music_paused=false"* ]] || fail "the mute / pause hold is not what the flags ask ($s)"
pass "the request carries its flags and holds the world muted, the music playing ($s)"
ask video.skip >/dev/null
wait_status 15 "video.skip to end the interruptable movie" "last: FNVIntro.bik Played \(ended early\)"
[[ "$(status)" == *"not playing"* ]] || fail "still playing after video.skip ($(status))"
pass "video.skip ended the interruptable movie as Played (ended early)"

# ── 2 — not interruptable ────────────────────────────────────────────────────
ask "obscript.cmd none 1114 t:FNVIntro.bik 0" >/dev/null
wait_status 30 "the second movie playing" "playing FNVIntro.bik frame [1-9][0-9]*/.*interruptable=false"
before=$(status | grep -oE 'frame [0-9]+' | grep -oE '[0-9]+')
ask video.skip >/dev/null
sleep 3
after=$(status)
[[ "$after" == *"playing FNVIntro.bik"* ]] || fail "video.skip ended a movie that is not interruptable ($after)"
now=$(grep -oE 'frame [0-9]+' <<<"$after" | grep -oE '[0-9]+')
(( now > before )) || fail "the movie stopped advancing after video.skip (frame $before -> $now)"
grep -q "Escape ignored — 'FNVIntro.bik' is not interruptable" "$LOG_DIR/engine.stderr" || fail "the ignored Escape was not logged"
pass "video.skip did not end the non-interruptable movie; it kept playing (frame $before -> $now)"
echo "smoke[bik-flags]: PASS -- PlayBink keeps its flags"
