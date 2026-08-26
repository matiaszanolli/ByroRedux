#!/usr/bin/env bash
# M48 archive-backed menu smoke test — verify the `--menu` route can open a
# vanilla Bethesda menu out of a real game archive, end to end, through the
# shipped CLI entry point.
#
# Why this exists (#3273): `#3147` fixed the route, and the fix is correct by
# static reading, but its only automated coverage is `archive_menu_route_tests`
# — a unit test over `archive_menu_args`, the pure CLI-argument parser. Every
# part that can actually fail on real data sits past that parser and needs a
# Vulkan device:
#
#     Archive::open -> archive.extract -> ScaleformProfile::detect
#       -> load_swf_from_resource_provider -> register_rgba
#
# so `cargo test` cannot reach it. Without this gate, #3147's HIGH claim ("the
# shipped binary cannot open any vanilla Bethesda menu") only degrades to
# "unverified whether the shipped binary can open one" — a materially different
# and much weaker statement. #2968's redundant-decompress path and the whole
# `ScaleformNavigatorRuntime` preload loop are also exercised for the first time
# by a real caller on this route, so they ride on the same gate.
#
# Follows the `--bench-hold` -> `byro-dbg`-attach pattern documented in
# docs/smoke-tests/README.md, same as m41-equip.sh.
#
# Workflow:
#   1. Launch the engine in the background under `--menu <swf>
#      --menu-archive <ba2|bsa> --bench-frames N --bench-hold`, so the bench
#      summary lands and the debug server stays reachable afterwards.
#   2. Wait for the `bench-hold:` notice on stderr.
#   3. Assert the route's success token (`ui.menu: loaded`) is present and that
#      none of the route's five failure arms logged.
#   4. Attach `byro-dbg` and confirm the engine is live and serving, so a
#      launch that logged success and then died is not scored a PASS.
#   5. SIGTERM the engine.
#
# Usage:
#   docs/smoke-tests/m48-menu-load.sh [fo4|skyrim|all]
#
# Exit: 0 PASS, 77 SKIP (game data absent), non-zero FAIL.

set -euo pipefail

GAME="${1:-all}"

SKYRIM_DATA="${BYROREDUX_SKYRIM_DATA:-/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data}"
FO4_DATA="${BYROREDUX_FO4_DATA:-/mnt/data/SteamLibrary/steamapps/common/Fallout 4/Data}"

PORT="${BYRO_DEBUG_PORT:-9876}"
BENCH_FRAMES="${BYROREDUX_SMOKE_FRAMES:-30}"

LOG_DIR="$(mktemp -d)"
trap 'rm -rf "$LOG_DIR"' EXIT

fail() {
    echo "smoke[m48-menu-load]: FAIL -- $*" >&2
    exit 1
}

# Args: $1 = label, $2 = archive path, $3 = archive-relative SWF path.
run_menu () {
    local label="$1"
    local archive="$2"
    local swf="$3"
    local engine_log="$LOG_DIR/$label.engine.log"
    local dbg_log="$LOG_DIR/$label.dbg.log"

    echo "═══════════════════════════════════════════════════════════════"
    echo "  smoke[m48-menu-load/$label]: $swf out of $(basename "$archive")"
    echo "═══════════════════════════════════════════════════════════════"

    cargo run --release --quiet -- \
        --menu "$swf" \
        --menu-archive "$archive" \
        --bench-frames "$BENCH_FRAMES" \
        --bench-hold \
        > "$engine_log.stdout" 2> "$engine_log.stderr" &
    local engine_pid=$!

    local kill_engine='kill -TERM "$engine_pid" 2>/dev/null || true; wait "$engine_pid" 2>/dev/null || true'

    # Poll for the bench-hold notice rather than sleeping a fixed span: a
    # cold `cargo run --release` link plus archive open is not a predictable
    # duration, and a fixed sleep either flakes or wastes a minute.
    local waited=0
    while ! grep -Fq "bench-hold:" "$engine_log.stderr" 2>/dev/null; do
        if ! kill -0 "$engine_pid" 2>/dev/null; then
            echo "--- engine stderr ---" >&2
            tail -40 "$engine_log.stderr" >&2
            fail "$label: engine exited before reaching bench-hold"
        fi
        sleep 1
        waited=$((waited + 1))
        if (( waited > 300 )); then
            eval "$kill_engine"
            fail "$label: no bench-hold notice after ${waited}s"
        fi
    done

    # The success token. `#3273` added it precisely so this gate has a
    # positive observable — before it, the route logged only on failure and
    # a silent run was indistinguishable from a working one.
    if ! grep -Fq "ui.menu: loaded" "$engine_log.stderr"; then
        eval "$kill_engine"
        echo "--- engine stderr ---" >&2
        tail -40 "$engine_log.stderr" >&2
        fail "$label: route never reported a loaded menu"
    fi

    # Each of the five failure arms in the `--menu` route. Checked
    # individually rather than as one 'ERROR' grep so a FAIL names the stage
    # that broke: archive open, extract, profile detect, SWF load, or the
    # texture registration.
    local arm
    for arm in \
        "Failed to open UI archive" \
        "Failed to extract archive menu" \
        "Failed to detect Scaleform profile" \
        "Failed to load archive menu" \
        "Failed to register UI texture"; do
        if grep -Fq "$arm" "$engine_log.stderr"; then
            eval "$kill_engine"
            echo "--- engine stderr ---" >&2
            grep -F "$arm" "$engine_log.stderr" >&2
            fail "$label: $arm"
        fi
    done

    # A launch that logged success and then died is not a pass. Attaching
    # byro-dbg proves the process is still serving after the bench window.
    if ! printf 'stats\n' | cargo run --release --quiet -p byro-dbg > "$dbg_log" 2>&1; then
        eval "$kill_engine"
        echo "--- byro-dbg output ---" >&2
        cat "$dbg_log" >&2
        fail "$label: byro-dbg could not attach on port $PORT after bench-hold"
    fi

    eval "$kill_engine"
    echo "smoke[m48-menu-load/$label]: PASS -- menu loaded and engine stayed live"
}

ran=0

if [[ "$GAME" == "fo4" || "$GAME" == "all" ]]; then
    archive="$FO4_DATA/Fallout4 - Interface.ba2"
    if [[ -f "$archive" ]]; then
        run_menu fo4 "$archive" 'interface\hudmenu.swf'
        ran=$((ran + 1))
    else
        echo "smoke[m48-menu-load]: SKIP -- missing $archive"
        [[ "$GAME" == "fo4" ]] && exit 77
    fi
fi

if [[ "$GAME" == "skyrim" || "$GAME" == "all" ]]; then
    archive="$SKYRIM_DATA/Skyrim - Interface.bsa"
    if [[ -f "$archive" ]]; then
        run_menu skyrim "$archive" 'interface\hudmenu.swf'
        ran=$((ran + 1))
    else
        echo "smoke[m48-menu-load]: SKIP -- missing $archive"
        [[ "$GAME" == "skyrim" ]] && exit 77
    fi
fi

if (( ran == 0 )); then
    echo "smoke[m48-menu-load]: SKIP -- missing game data for every requested title"
    exit 77
fi

echo "smoke[m48-menu-load]: PASS -- $ran menu route(s) verified"
