#!/usr/bin/env bash
# Cross-cell actor travel, PRD scenario 3 (XCT-LOC-004 / XCT-LOC-005, #6852): a scripted
# MoveTo of an actor whose cell is not loaded, to a reference in another space.
#
#   1. Boot in Goodsprings (WastelandNV) with the Prospector Saloon unloaded.
#   2. SunnyRef (0x00104E85, persistent in the saloon) still has a logical identity: actor.where
#      answers for her, and the console resolves her as an ObScript caller.
#   3. `obscript.cmd 0x00104E85 109e r:0010A201` (MoveTo VCG02SunnyOutsideMarkerREF) moves her
#      through the relocation ledger: actor.where reports her in WastelandNV, at the marker.
#
# Headless (Xvfb). FNV only. See docs/prd/cross-cell-actor-travel.md §7.2 and
# docs/engine/cross-cell-travel.md §6.
#   docs/smoke-tests/xct-scripted-move.sh

set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/fixture.sh"
smoke_load_fixture xct-scripted-move fnv
smoke_require_fixture_fields VR_SUNNY
smoke_require_data

SUNNY="$VR_SUNNY"
MARKER=0010A201
ROOT_DIR="$SMOKE_ROOT_DIR"
ENGINE_BIN="$ROOT_DIR/target/release/byroredux"
DEBUG_BIN="$ROOT_DIR/target/release/byro-dbg"
PORT="${BYRO_DEBUG_PORT:-9876}"

LOG_DIR="$(mktemp -d /tmp/byro-xct-scripted-move.XXXXXX)"
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
    echo "smoke[xct-scripted-move]: FAIL -- $*"
    echo "smoke[xct-scripted-move]: artifacts retained at $LOG_DIR"
    tail -30 "$LOG_DIR/engine.stderr" 2>/dev/null || true
    exit 1
}
pass() { echo "smoke[xct-scripted-move]: PASS -- $*"; }

if [[ ! -x "$ENGINE_BIN" || ! -x "$DEBUG_BIN" ]]; then
    echo "smoke[xct-scripted-move]: building release binaries"
    (cd "$ROOT_DIR" && cargo build --release --quiet -p byroredux -p byro-dbg)
fi

ask() {
    { printf '%s\n' "$1"; echo .quit; } | env BYRO_DEBUG_PORT="$PORT" "$DEBUG_BIN" >"$LOG_DIR/ask.tmp" 2>&1
    sed 's/\\n/\n/g; s/\\"/"/g' "$LOG_DIR/ask.tmp"
}
wait_for() {
    local seconds="$1" description="$2" command="$3" pattern="$4" deadline=$((SECONDS + $1))
    until ask "$command" | grep -Eiq -- "$pattern"; do
        kill -0 "$engine_pid" 2>/dev/null || fail "the engine exited waiting for: $description"
        (( SECONDS < deadline )) || fail "timeout (${seconds}s) waiting for: $description ($(ask "$command" | tail -2 | tr '\n' ' ' | cut -c1-300))"
        sleep 2
    done
}

cd "$SMOKE_DATA"
env BYRO_DEBUG_PORT="$PORT" BYRO_DEBUG_SERVER=1 \
    BYROREDUX_SETTINGS_PATH="$LOG_DIR/settings.toml" \
    BYROREDUX_SAVE_DIR="$LOG_DIR/saves" \
    RUST_LOG="warn,byroredux::app_step=info,byroredux::cell_loader::relocated=info,byroredux::cell_loader::created=info" \
    "${SMOKE_HEADLESS[@]}" "$ENGINE_BIN" "${SMOKE_ENGINE_ARGS[@]}" \
    --wrld WastelandNV --grid -17,0 --radius 2 --bench-frames 30 --bench-hold \
    >"$LOG_DIR/engine.stdout" 2>"$LOG_DIR/engine.stderr" &
engine_pid=$!

deadline=$((SECONDS + 300))
until grep -q 'bench-hold:' "$LOG_DIR/engine.stderr" 2>/dev/null; do
    kill -0 "$engine_pid" 2>/dev/null || fail "engine exited before bench-hold"
    (( SECONDS < deadline )) || fail "timeout waiting for bench-hold"
    sleep 0.5
done
pass "engine reached Goodsprings, the saloon unloaded"

# ── 2 — Sunny's logical identity ─────────────────────────────────────────────
wait_for 60 "Sunny not loaded, in the saloon" "actor.where $SUNNY" "not loaded, in Interior .*GSProspectorSaloonInterior"
ask "obscript.cmd $SUNNY 109e r:$MARKER" >"$LOG_DIR/move.out"
# A carried-out command with no return value answers `0x109e -> (no answer)`; a decline or an
# unresolved caller answers otherwise.
grep -q '0x109e -> (no answer)' "$LOG_DIR/move.out" || fail "the console could not run MoveTo on Sunny ($(grep 'byro>' "$LOG_DIR/move.out" | head -2 | tr '\n' ' '))"
pass "Sunny resolves as an ObScript caller with her cell unloaded (XCT-LOC-005)"

# ── 3 — she is moved through the ledger to the marker ────────────────────────
wait_for 60 "Sunny in WastelandNV" "actor.where $SUNNY" ": in Exterior \{ worldspace: \"wastelandnv\".*moved from its home cell"
where=$(ask "actor.where $SUNNY" | grep -o 'at (.*)' | head -1)
ask "mesh.info 0x$MARKER" >"$LOG_DIR/marker.info"
marker=$(grep -o 'Transform.global: *pos *([^)]*)' "$LOG_DIR/marker.info" | grep -oE '[-+]?[0-9]+(\.[0-9]+)?' | head -3 | tr '\n' ' ')
read -r sx sy sz <<<"$(grep -oE '[-+]?[0-9]+' <<<"$where" | tr '\n' ' ')"
read -r mx my mz <<<"$marker"
[[ -n "${mz:-}" && -n "${sz:-}" ]] || fail "no positions to compare (sunny: $where, marker: $marker)"
awk -v sx="$sx" -v sz="$sz" -v mx="$mx" -v mz="$mz" 'BEGIN { exit !((sx - mx) ^ 2 + (sz - mz) ^ 2 < 256 ^ 2) }' ||
    fail "Sunny is not at the marker (sunny $where, marker $marker)"
pass "Sunny is in WastelandNV at VCG02SunnyOutsideMarkerREF ($where)"
echo "smoke[xct-scripted-move]: PASS -- a scripted MoveTo moved an unloaded actor across spaces"
