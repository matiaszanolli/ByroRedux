#!/usr/bin/env bash
# Bink video playback, Phase 3 (BIK-REQ-004 / BIK-REQ-005, #6859) and PRD scenario 4 (BIK-DEC-003):
# a headless boot plays the game's start sequence to its last frame. The movies come from the
# install's default INI or the executable's compiled-in sIntroSequence / sMainMenuMovieIntro
# (docs/engine/video-playback.md §5): Oblivion's four logos and its menu intro (Bink 1), Skyrim's
# BGS_Logo.bik, Fallout 4's GameIntro_V3_B.bk2 (Bink 2, six audio tracks: the first plays).
#
#   docs/smoke-tests/bik-start-sequence.sh [oblivion|skyrim_se|fo4]   (default oblivion)
#
# Headless (Xvfb). A benchmark run skips the start sequence unless --start-movies asks for it.

set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/fixture.sh"
smoke_load_fixture bik-start-sequence "${1:-oblivion}"
smoke_require_fixture_fields P0_CELL
smoke_require_data

case "$SMOKE_GAME" in
    oblivion) MOVIES=("bethesda softworks HD720p.bik" "2k games.bik" "game studios.bik" "Oblivion Legal.bik" "Oblivion iv logo.bik") ;;
    skyrim_se) MOVIES=("BGS_Logo.bik") ;;
    fo4) MOVIES=("GameIntro_V3_B.bk2") ;;
    *) echo "smoke[bik-start-sequence]: FAIL -- no start sequence is gated for $SMOKE_GAME"; exit 2 ;;
esac

ROOT_DIR="$SMOKE_ROOT_DIR"
ENGINE_BIN="$ROOT_DIR/target/release/byroredux"
DEBUG_BIN="$ROOT_DIR/target/release/byro-dbg"
PORT="${BYRO_DEBUG_PORT:-9876}"
GATE="bik-start-sequence[$SMOKE_GAME]"

LOG_DIR="$(mktemp -d /tmp/byro-bik-start.XXXXXX)"
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
    echo "smoke[$GATE]: FAIL -- $*"
    echo "smoke[$GATE]: artifacts retained at $LOG_DIR"
    grep 'video:' "$LOG_DIR/engine.stderr" 2>/dev/null | tail -12 || true
    exit 1
}
pass() { echo "smoke[$GATE]: PASS -- $*"; }

if [[ ! -x "$ENGINE_BIN" || ! -x "$DEBUG_BIN" ]]; then
    echo "smoke[$GATE]: building release binaries"
    (cd "$ROOT_DIR" && cargo build --release --quiet -p byroredux -p byro-dbg)
fi

ask() {
    { printf '%s\n' "$1"; echo .quit; } | env BYRO_DEBUG_PORT="$PORT" "$DEBUG_BIN" >"$LOG_DIR/ask.tmp" 2>&1
    sed 's/\\n/\n/g; s/\\"/"/g' "$LOG_DIR/ask.tmp"
}

cd "$SMOKE_DATA"
env BYRO_DEBUG_PORT="$PORT" BYRO_DEBUG_SERVER=1 \
    BYROREDUX_SETTINGS_PATH="$LOG_DIR/settings.toml" \
    BYROREDUX_SAVE_DIR="$LOG_DIR/saves" \
    RUST_LOG="warn,byroredux::video=info" \
    "${SMOKE_HEADLESS[@]}" "$ENGINE_BIN" "${SMOKE_ENGINE_ARGS[@]}" \
    --cell "$P0_CELL" --start-movies --bench-frames 30 --bench-hold \
    >"$LOG_DIR/engine.stdout" 2>"$LOG_DIR/engine.stderr" &
engine_pid=$!

deadline=$((SECONDS + 300))
until grep -q 'bench-hold:' "$LOG_DIR/engine.stderr" 2>/dev/null; do
    kill -0 "$engine_pid" 2>/dev/null || fail "engine exited before bench-hold"
    (( SECONDS < deadline )) || fail "timeout waiting for bench-hold"
    sleep 0.5
done
pass "engine held in $P0_CELL"

grep -q "video: start sequence" "$LOG_DIR/engine.stderr" || fail "no start sequence was queued"
pass "$(grep -o 'video: start sequence.*' "$LOG_DIR/engine.stderr" | head -1)"

# Each movie plays to its last frame: Played, not ended early, in order.
last="${MOVIES[${#MOVIES[@]}-1]}"
deadline=$((SECONDS + 240))
until [[ "$(ask video.status | grep -o 'last: .*' | head -1)" == *"last: $last Played"* ]]; do
    kill -0 "$engine_pid" 2>/dev/null || fail "the engine exited during the start sequence"
    (( SECONDS < deadline )) || fail "the start sequence did not finish ($(ask video.status | grep -o 'video: .*' | head -1))"
    sleep 2
done
for movie in "${MOVIES[@]}"; do
    grep -Fq "video: '$movie' Played { ended_early: false }" "$LOG_DIR/engine.stderr" ||
        fail "'$movie' did not play to its last frame ($(grep -F "'$movie'" "$LOG_DIR/engine.stderr" | tail -1))"
    frames=$(grep -F "video: '$movie' Played" "$LOG_DIR/engine.stderr" | grep -o 'after [0-9]* frame(s)' | head -1)
    pass "'$movie' played to its last frame ($frames)"
done
if [[ "$SMOKE_GAME" == fo4 ]]; then
    grep -q "video: playing 'GameIntro_V3_B.bk2' .*6 audio track(s), soundtrack=yes" "$LOG_DIR/engine.stderr" ||
        fail "the Bink 2 movie did not bring its first audio track ($(grep -o "video: playing 'GameIntro_V3_B.bk2'.*" "$LOG_DIR/engine.stderr" | head -1))"
    pass "the Bink 2 movie played with its first of six audio tracks (scenario 4)"
fi
echo "smoke[$GATE]: PASS -- the start sequence played to its last frame"
