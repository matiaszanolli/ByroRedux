#!/usr/bin/env bash
# Cross-cell actor travel, PRD scenario 4 (#6854): the coverage sweep. Every unloaded actor with
# a package whose destination is in another space is sent on its way offscreen (`xct.sweep`); the
# clock is advanced past every arrival; `actor.where --census` must then list every swept actor as
# Arrived or Unroutable with its reason (none still offscreen or routing). Prints the arrival rate
# ROADMAP's M42 row publishes.
#
#   docs/smoke-tests/xct-census.sh [fnv|fo3|oblivion|skyrim_se|fo4]   (default fnv)
#
# Headless (Xvfb). Boots the game's P0 cell. See docs/prd/cross-cell-actor-travel.md §7.2 / §8
# and docs/engine/cross-cell-travel.md §6.

set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/fixture.sh"
smoke_load_fixture xct-census "${1:-fnv}"
smoke_require_fixture_fields P0_CELL
smoke_require_data

ROOT_DIR="$SMOKE_ROOT_DIR"
ENGINE_BIN="$ROOT_DIR/target/release/byroredux"
DEBUG_BIN="$ROOT_DIR/target/release/byro-dbg"
PORT="${BYRO_DEBUG_PORT:-9876}"
GATE="xct-census[$SMOKE_GAME]"

LOG_DIR="$(mktemp -d /tmp/byro-xct-census.XXXXXX)"
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
    tail -30 "$LOG_DIR/engine.stderr" 2>/dev/null || true
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
    RUST_LOG="warn" \
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

ask "xct.sweep" >"$LOG_DIR/sweep.out"
sweep=$(grep -o 'xct.sweep: .*' "$LOG_DIR/sweep.out" | head -1 | sed 's/"$//')
[[ -n "$sweep" ]] || fail "xct.sweep gave no answer ($(head -3 "$LOG_DIR/sweep.out" | tr '\n' ' '))"
swept=$(grep -oE '[0-9]+ actor\(s\) swept' <<<"$sweep" | grep -oE '^[0-9]+')
(( swept > 0 )) || fail "nothing to sweep: $sweep"
pass "$sweep"

# Every journey's walk time is hours at most; two game days is past them all.
ask "time.advance 48" >/dev/null
deadline=$((SECONDS + 180))
until ask "actor.where --census" | grep -Eq 'offscreen 0, routing 0,'; do
    kill -0 "$engine_pid" 2>/dev/null || fail "the engine exited before every journey ended"
    (( SECONDS < deadline )) || fail "journeys still pending: $(ask 'actor.where --census' | grep -o 'census: .*' | head -1)"
    sleep 3
done
census=$(ask "actor.where --census" | grep -o 'census: .*' | head -1)
arrived=$(grep -oE 'arrived [0-9]+' <<<"$census" | grep -oE '[0-9]+')
pass "$census"
awk -v a="$arrived" -v s="$swept" 'BEGIN { printf "smoke['"$GATE"']: arrival rate %d / %d = %.1f%%\n", a, s, 100 * a / s }'
echo "smoke[$GATE]: PASS -- every swept destination is Arrived or Unroutable with its reason"
