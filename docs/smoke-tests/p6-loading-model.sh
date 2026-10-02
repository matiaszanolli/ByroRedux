#!/usr/bin/env bash
# Creation-era loading-screen gate — drive each Creation-era title's P0
# door route and verify the LSCR model cover end-to-end:
#
#   1. `loadscreen.census` reports eligible screens on the live index and
#      names a cover pick with resolvable art (model path, not an image).
#   2. The door transition begins the cover: `loading.screen: begin` +
#      `model stage … meshes=N` (N ≥ 1) + presented before teardown.
#   3. A screenshot captured while the cover is up is a rendered frame,
#      not a blank/black one (size floor calibrated on vanilla data).
#   4. The destination applies and the cover dismisses after one coherent
#      destination frame — the door never blocks on the cover.
#
# Skyrim exercises the inline SNAM/RNAM/XNAM pose lane (transform source
# `inline`); FO4 exercises the TNAM → TRNS lane (`trns=<EDID>`).
#
# Runs BOTH titles in one invocation (that is the point of this gate);
# pass a subset: p6-loading-model.sh skyrim_se | p6-loading-model.sh fo4.
#
# Pre-conditions: Skyrim SE + FO4 installed (BYROREDUX_SKYRIMSE_DATA /
# BYROREDUX_FO4_DATA or the default paths), Vulkan device, Xvfb.

set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/fixture.sh"

GAMES="${1:-skyrim_se fo4}"
if [[ -n "${BYROREDUX_SMOKE_GAME:-}" ]]; then
    GAMES="$BYROREDUX_SMOKE_GAME"
fi

PORT="${BYRO_DEBUG_PORT:-19884}"
BENCH_FRAMES="${BYROREDUX_SMOKE_FRAMES:-30}"
TIMEOUT="${BYROREDUX_SMOKE_TIMEOUT:-300}"
# Calibrated on vanilla captures 2026-10-02: a rendered model + tip frame
# is > 100 KB; a pure-black failure frame compresses below 20 KB.
MIN_SHOT_BYTES="${BYROREDUX_SMOKE_MIN_SHOT_BYTES:-30000}"

LOG_DIR="$(mktemp -d /tmp/byro-p6-loadmodel.XXXXXX)"
engine_pid=""
cleanup() {
    if [[ -n "$engine_pid" ]]; then
        kill -TERM "$engine_pid" 2>/dev/null || true
        wait "$engine_pid" 2>/dev/null || true
    fi
    rm -rf "$LOG_DIR"
}
trap cleanup EXIT

fail() {
    echo "smoke[p6-loading-model]: FAIL -- $*" >&2
    echo "artifacts: $LOG_DIR" >&2
    exit 1
}

# Binary-crate toolchain (#4466): the bin crate needs rustc >= 1.94 and the
# distro cargo (1.93.1) cannot resolve cranelift's MSRV — use the installed
# 1.96.0 toolchain's cargo directly, same invocation as CLAUDE.md.
TC="$(rustup which --toolchain 1.96.0 cargo)"
PATH="$(dirname "$TC"):$PATH" "$TC" build --release --offline --quiet -p byroredux -p byro-dbg

debug_command() {
    timeout 60 env BYRO_DEBUG_PORT="$PORT" target/release/byro-dbg >"$2" 2>&1 <<EOF
$1
.quit
EOF
}

overall=0

for game in $GAMES; do
    echo "================================================================"
    echo "  smoke[p6-loading-model]: $game"
    echo "================================================================"
    smoke_load_fixture p6-loading-model "$game"
    smoke_require_fixture_fields \
        P0_CELL P0_CAMERA_POS P0_CAMERA_FORWARD P0_APPLIED_LOG P0_ENTITY_FLOOR
    smoke_require_data

    # Skyrim's vanilla load-screen art lives in Meshes1 (Meshes0 only has
    # the DLC copies); the base fixture archive set stops at Meshes0.
    extra_args=()
    if [[ "$game" == "skyrim_se" ]]; then
        extra_args+=(--bsa "Skyrim - Meshes1.bsa")
    fi

    engine_stderr="$LOG_DIR/$game.engine.stderr"
    census_log="$LOG_DIR/$game.census.debug.log"
    shot="$LOG_DIR/$game.cover.png"
    cd "$SMOKE_ROOT_DIR"
    rm -f "screenshots/p6-cover.png"
    BYRO_DEBUG_PORT="$PORT" BYRO_DEBUG_SERVER=1 \
    RUST_LOG="warn,byroredux::loading_screen=info,byroredux::cell_loader::transition=info,byroredux::interaction=info" \
    target/release/byroredux \
        "${SMOKE_ENGINE_ARGS[@]}" \
        "${extra_args[@]}" \
        --cell "$P0_CELL" \
        --fly \
        --camera-pos "$P0_CAMERA_POS" \
        --camera-forward "$P0_CAMERA_FORWARD" \
        --bench-frames "$BENCH_FRAMES" \
        --bench-hold \
        >"$LOG_DIR/$game.engine.stdout" 2>"$engine_stderr" &
    engine_pid=$!

    deadline=$(( $(date +%s) + TIMEOUT ))
    while ! grep -q '^bench-hold:' "$engine_stderr" 2>/dev/null; do
        if [[ $(date +%s) -gt $deadline ]]; then
            fail "$game: timeout waiting for bench-hold"
        fi
        kill -0 "$engine_pid" 2>/dev/null || fail "$game: engine exited before bench-hold"
        sleep 0.5
    done

    gate() {
        if grep -Fq "$2" "$1"; then
            echo "smoke[p6-loading-model]: PASS -- $3"
        else
            echo "smoke[p6-loading-model]: FAIL -- $3 (missing '$2')" >&2
            overall=1
        fi
    }

    # 1. Live census: eligible screens exist and name resolvable art.
    debug_command "loadscreen.census" "$census_log"
    if grep -Eq 'eligible=[1-9][0-9]*' "$census_log"; then
        echo "smoke[p6-loading-model]: PASS -- $game census has eligible screens"
    else
        echo "smoke[p6-loading-model]: FAIL -- $game census eligible=0" >&2
        cat "$census_log" >&2 || true
        overall=1
    fi
    gate "$census_log" "cover pick: " "census names a deterministic cover pick"
    if grep -Eq 'cover pick: [0-9A-F]{8} .* art=.+' "$census_log"; then
        echo "smoke[p6-loading-model]: PASS -- cover pick art resolves"
    else
        echo "smoke[p6-loading-model]: FAIL -- cover pick art empty" >&2
        overall=1
    fi

    # 2. Drive the P0 door; the cover must begin + present.
    debug_command "input.press activate" "$LOG_DIR/$game.press.debug.log" || true
    deadline=$(( $(date +%s) + TIMEOUT ))
    while ! grep -Fq "loading.screen: model stage" "$engine_stderr" 2>/dev/null; do
        if [[ $(date +%s) -gt $deadline ]]; then
            fail "$game: timeout waiting for the model stage to begin"
        fi
        kill -0 "$engine_pid" 2>/dev/null || fail "$game: engine exited before the cover began"
        sleep 0.1
    done
    gate "$engine_stderr" "loading.screen: begin LSCR=" "cover began from a real LSCR"
    gate "$engine_stderr" "loading.screen: presented before scene teardown" "cover presented before teardown"

    # 3. Screenshot while the cover is up (bare name; the debug server
    # writes under the engine cwd's screenshots/).
    debug_command "screenshot p6-cover.png" "$LOG_DIR/$game.shot.debug.log" || true
    for attempt in $(seq 1 50); do
        [[ -s "screenshots/p6-cover.png" ]] && break
        sleep 0.2
    done
    if [[ -s "screenshots/p6-cover.png" ]]; then
        mv "screenshots/p6-cover.png" "$shot"
        shot_bytes="$(stat -c%s "$shot")"
        if (( shot_bytes >= MIN_SHOT_BYTES )); then
            echo "smoke[p6-loading-model]: PASS -- cover frame renders (${shot_bytes}B ≥ ${MIN_SHOT_BYTES}B)"
        else
            echo "smoke[p6-loading-model]: FAIL -- cover frame too small (${shot_bytes}B < ${MIN_SHOT_BYTES}B): blank?" >&2
            overall=1
        fi
    else
        echo "smoke[p6-loading-model]: FAIL -- cover screenshot never landed" >&2
        overall=1
    fi

    # 4. The destination applies and the cover dismisses after it.
    deadline=$(( $(date +%s) + TIMEOUT ))
    while ! grep -F "Cell transition applied:" "$engine_stderr" | grep -Fq "$P0_APPLIED_LOG"; do
        if [[ $(date +%s) -gt $deadline ]]; then
            fail "$game: timeout waiting for the authored transition"
        fi
        kill -0 "$engine_pid" 2>/dev/null || fail "$game: engine exited during transition"
        sleep 0.5
    done
    deadline=$(( $(date +%s) + 60 ))
    while ! grep -Fq "loading.screen: dismissed after destination frame" "$engine_stderr" 2>/dev/null; do
        if [[ $(date +%s) -gt $deadline ]]; then
            break # gate below reports it
        fi
        kill -0 "$engine_pid" 2>/dev/null || break
        sleep 0.2
    done
    gate "$engine_stderr" \
        "loading.screen: dismissed after destination frame" \
        "cover dismissed after one coherent destination frame"
    gate "$engine_stderr" "$P0_APPLIED_LOG" "authored destination applied"

    kill -TERM "$engine_pid" 2>/dev/null || true
    wait "$engine_pid" 2>/dev/null || true
    engine_pid=""
done

if (( overall != 0 )); then
    echo "smoke[p6-loading-model]: FAIL (see gates above); artifacts: $LOG_DIR" >&2
    exit 1
fi
echo "smoke[p6-loading-model]: PASS (both Creation-era titles gate the LSCR model cover)"
