#!/usr/bin/env bash
# xWMA Audio Decoding, PRD §7.2 scenario 3 and the loose-override half of scenario 7 (#7414):
# Fallout 4's effects decode through the static loader in the real engine, and a mod's `.xwm` replaces
# a vanilla one.
#
#   3. Booting Med-Tek Research with `Fallout4 - Sounds.ba2` loads the default footstep and water-splash
#      one-shots, both `.xwm` (XWMA-STAT-001), and `snd.failed` reports that no sound failed to decode
#      (XWMA-REP-002).
#   7. With a loose root whose `sound/fx/fst/.../fst_dirt_walk_player_03.xwm` is not a valid xWMA, the
#      loose file wins over the archive (XWMA-MOD-001), the default footstep fails to decode, and
#      `snd.failed` lists exactly that path with an xWMA reason (XWMA-REP-001, XWMA-REP-002).
#
# This gate proves the decode and the failure reporting on the real archive. It does not hear anything:
# audible playback (a footstep in the cell, the splash entering water) is a manual check on a machine
# with an audio device. It needs Vulkan, a display or `xvfb-run`, and Fallout 4's data
# (BYROREDUX_FO4_DATA); exit 77 when the data is missing.
#
#   docs/smoke-tests/xwma-effects.sh

set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/fixture.sh"
smoke_load_fixture xwma-effects fo4
smoke_require_data

ROOT_DIR="$SMOKE_ROOT_DIR"
ENGINE_BIN="$ROOT_DIR/target/release/byroredux"
DEBUG_BIN="$ROOT_DIR/target/release/byro-dbg"
PORT="${BYRO_DEBUG_PORT:-9876}"
FOOTSTEP_KEY='sound\fx\fst\player\dirt\walk\l\fst_dirt_walk_player_03.xwm'
SPLASH_KEY='sound\fx\phy\generic\phy_generic_water_medium_01.xwm'

LOG_DIR="$(mktemp -d /tmp/byro-xwma-effects.XXXXXX)"
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
    echo "smoke[xwma-effects]: FAIL -- $*"
    echo "smoke[xwma-effects]: artifacts retained at $LOG_DIR"
    grep -i -E 'xwma|footstep|water acoustics|decode' "$LOG_DIR"/engine.*.stderr 2>/dev/null | tail -10 || true
    exit 1
}
pass() { echo "smoke[xwma-effects]: PASS -- $*"; }

if [[ ! -x "$ENGINE_BIN" || ! -x "$DEBUG_BIN" ]]; then
    echo "smoke[xwma-effects]: building release binaries"
    (cd "$ROOT_DIR" && cargo build --release --quiet -p byroredux -p byro-dbg)
fi

ask() {
    { printf '%s\n' "$@"; echo .quit; } | env BYRO_DEBUG_PORT="$PORT" "$DEBUG_BIN" >"$LOG_DIR/ask.tmp" 2>&1
    sed 's/\\n/\n/g; s/\\"/"/g; s/\\\\/\\/g' "$LOG_DIR/ask.tmp"
}

# start_engine <label> <extra engine args...>: launch headless and wait for the bench hold.
start_engine() {
    local label="$1"
    shift
    stop_engine
    (
        cd "$SMOKE_DATA"
        exec env BYRO_DEBUG_PORT="$PORT" BYRO_DEBUG_SERVER=1 \
            BYROREDUX_SETTINGS_PATH="$LOG_DIR/settings.toml" \
            BYROREDUX_SAVE_DIR="$LOG_DIR/saves" \
            RUST_LOG="warn,byroredux::asset_provider=info" \
            "${SMOKE_HEADLESS[@]}" "$ENGINE_BIN" "${SMOKE_ENGINE_ARGS[@]}" --cell "$P0_CELL" "$@" \
            --bench-frames 30 --bench-hold
    ) >"$LOG_DIR/engine.$label.stdout" 2>"$LOG_DIR/engine.$label.stderr" &
    engine_pid=$!
    local deadline=$((SECONDS + 300))
    until grep -q 'bench-hold:' "$LOG_DIR/engine.$label.stderr" 2>/dev/null; do
        kill -0 "$engine_pid" 2>/dev/null || fail "engine exited before bench-hold ($label)"
        (( SECONDS < deadline )) || fail "timeout waiting for bench-hold ($label)"
        sleep 0.5
    done
}

# ── 3 — the vanilla .xwm defaults load, and nothing failed ───────────────────
start_engine vanilla
stderr="$LOG_DIR/engine.vanilla.stderr"
grep -F "footstep sound loaded from --sounds-bsa ('$FOOTSTEP_KEY" "$stderr" >/dev/null \
    || fail "the default footstep ($FOOTSTEP_KEY) did not load from the archive"
pass "the default footstep decoded from the archive: $FOOTSTEP_KEY"
grep -F "water acoustics: loaded '$SPLASH_KEY" "$stderr" >/dev/null \
    || fail "the default splash ($SPLASH_KEY) did not load from the archive"
pass "the default water splash decoded from the archive: $SPLASH_KEY"
failed="$(ask snd.failed)"
[[ "$failed" == *"No sound failed to decode"* ]] \
    || fail "snd.failed lists failures on an unmodified install: $(tr '\n' ' ' <<<"$failed" | cut -c1-300)"
pass "snd.failed: No sound failed to decode"

# ── 7 — a loose .xwm replaces the archive's, and its failure is listed ───────
loose="$LOG_DIR/loose-root"
mkdir -p "$loose/sound/fx/fst/player/dirt/walk/l"
# Not an xWMA (a RIFF header with no fmt chunk): a mod's broken file, in the vanilla file's place.
printf 'RIFF\004\000\000\000XWMA' >"$loose/${FOOTSTEP_KEY//\\//}"
start_engine loose --loose-root "$loose"
failed="$(ask snd.failed)"
grep -F -- "$FOOTSTEP_KEY" <<<"$failed" | grep -qi 'xwma' \
    || fail "the loose file did not replace the archive's, or its failure is not listed: $(tr '\n' ' ' <<<"$failed" | cut -c1-300)"
pass "the loose root's file won over the archive and snd.failed lists it: $(grep -F -- "$FOOTSTEP_KEY" <<<"$failed" | head -1)"
grep -F "water acoustics: loaded '$SPLASH_KEY" "$LOG_DIR/engine.loose.stderr" >/dev/null \
    || fail "an unrelated vanilla .xwm (the splash) stopped loading with a loose root present"
pass "the vanilla splash still decodes beside it"

echo "smoke[xwma-effects]: PASS -- scenario 3 (decode and listing) and the loose-override half of scenario 7"
echo "smoke[xwma-effects]: not covered here: hearing the effects; walk in the cell and enter water on a machine with an audio device"
