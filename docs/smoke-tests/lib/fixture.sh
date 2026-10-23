#!/usr/bin/env bash
# Shared fixture loader for the playable-slice gates (#3039).
#
# Before this existed, `p0-door-interaction.sh`, `p1-character-traversal.sh`
# and `p2-melee-core.sh` each hard-coded `SKYRIM_DATA`, Whiterun's cell /
# camera pose / route, and Bleak Falls' frozen FormIDs inline — so covering a
# second title meant copying three scripts. They now read every game-specific
# value from `docs/smoke-tests/fixtures/<game>.env`, and adding a title costs
# one fixture file.
#
# Fixture values are *derived*, not guessed:
#   cargo run -p byroredux-plugin --example probe_slice_fixture -- <ESM> <CELL>
#     → the cell's REFR count, its teleport doors, each door's resolved
#       destination, and a camera pose aimed at it (Y-up, paste-ready).
#   cargo run -p byroredux-plugin --example probe_combat_fixture -- <ESM> <CELL>
#     → the melee gate's NPC reference/base pair, derived Health and the
#       resolved weapon leaves.
#
# Usage from a gate script:
#   source "$(dirname "${BASH_SOURCE[0]}")/lib/fixture.sh"
#   smoke_load_fixture p0-door-interaction "$@"
#   smoke_require_data              # SKIP 77 when the game's data is absent
#   ... "${SMOKE_ENGINE_ARGS[@]}" ...

set -euo pipefail

SMOKE_LIB_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SMOKE_TESTS_DIR="$(cd "$SMOKE_LIB_DIR/.." && pwd)"
SMOKE_ROOT_DIR="$(cd "$SMOKE_TESTS_DIR/../.." && pwd)"
SMOKE_FIXTURE_DIR="$SMOKE_TESTS_DIR/fixtures"

# Resolve the requested game, load its fixture, and compose the engine's
# data-path arguments.
#
#   $1  gate name, used only in diagnostics ("p0-door-interaction")
#   $2… the gate's own argv; the first positional (if any) selects the game
#
# Game selection order: `$2` → `$BYROREDUX_SMOKE_GAME` → `skyrim_se`. The
# default keeps every pre-#3039 invocation (and the CI workflow) working
# unchanged.
smoke_load_fixture() {
    SMOKE_GATE="$1"
    shift

    local requested="${1:-${BYROREDUX_SMOKE_GAME:-skyrim_se}}"
    # `skyrim` is the name every script and doc used before the fixture split;
    # the engine profile key is `skyrim_se`.
    case "$requested" in
        skyrim) requested=skyrim_se ;;
    esac
    SMOKE_GAME="$requested"

    local fixture="$SMOKE_FIXTURE_DIR/$SMOKE_GAME.env"
    if [[ ! -f "$fixture" ]]; then
        echo "smoke[$SMOKE_GATE]: FAIL -- no fixture for game '$SMOKE_GAME'" >&2
        echo "  available: $(cd "$SMOKE_FIXTURE_DIR" && ls *.env | sed 's/\.env$//' | tr '\n' ' ')" >&2
        exit 2
    fi
    # Older complete fixtures predate explicit per-gate coverage. New titles
    # can declare a partial measured route without inventing other fixtures.
    unset FIXTURE_GATES
    # shellcheck source=/dev/null
    source "$fixture"

    if declare -p FIXTURE_GATES &>/dev/null; then
        local supported=0 gate
        for gate in "${FIXTURE_GATES[@]}"; do
            [[ "$gate" == "$SMOKE_GATE" ]] && supported=1
        done
        if (( supported == 0 )); then
            echo "smoke[$SMOKE_GATE]: FAIL -- fixture '$SMOKE_GAME' has no measured route for this gate" >&2
            exit 2
        fi
    fi

    # Per-game data directory: the fixture names its override variable
    # (BYROREDUX_SKYRIMSE_DATA, BYROREDUX_FNV_DATA, …) and its canonical
    # default, matching the table in docs/smoke-tests/README.md.
    SMOKE_DATA="${!FIXTURE_DATA_ENV:-$FIXTURE_DATA_DEFAULT}"

    SMOKE_ENGINE_ARGS=(--esm "$SMOKE_DATA/$FIXTURE_ESM")
    local arg
    for arg in "${FIXTURE_ARCHIVE_ARGS[@]}"; do
        case "$arg" in
            --*) SMOKE_ENGINE_ARGS+=("$arg") ;;
            *) SMOKE_ENGINE_ARGS+=("$SMOKE_DATA/$arg") ;;
        esac
    done
}

# The headless display prefix for a gate that renders off-screen. `xvfb-run`
# alone still opens the window on the user's Wayland session (winit prefers
# Wayland when WAYLAND_DISPLAY is set): the capture then follows the
# compositor's sizing, and the user's real mouse and keyboard reach the gate's
# engine — a click on a dialogue topic or a held key moves the route.
SMOKE_HEADLESS=(env -u WAYLAND_DISPLAY -u XDG_SESSION_TYPE xvfb-run -a)

# Hard SKIP (exit 77) when the selected game's data is not installed. Never a
# pass — `scripts/check-playable-smoke-contracts.sh` pins that distinction.
smoke_require_data() {
    local required
    for required in "${FIXTURE_REQUIRED_FILES[@]}"; do
        if [[ ! -f "$SMOKE_DATA/$required" ]]; then
            echo "smoke[$SMOKE_GATE]: SKIP -- missing $SMOKE_DATA/$required"
            exit 77
        fi
    done
}

# Fixture fields that a gate cannot run without. A fixture that leaves one
# empty is a fixture bug, and must fail loudly rather than silently skipping
# the check that field gates (SKIP != PASS applies to fixtures too).
smoke_require_fixture_fields() {
    local field
    for field in "$@"; do
        if [[ -z "${!field:-}" ]]; then
            echo "smoke[$SMOKE_GATE]: FAIL -- fixture $SMOKE_GAME.env does not set $field" >&2
            exit 2
        fi
    done
}

# End a conversation a fixture's boot opened on its own (#6052). Prospector
# Saloon's Sunny Smiles carries an authored force-greet PACK that opens a
# GREETING with no activation. While the dialogue page is open the frame loop
# releases injected gameplay input every frame, so a gate's `input.hold` /
# `input.press` would be cleared and the character would never move.
#
# Needs the caller's `debug_command <commands> <logfile>` (every gate defines
# its own over byro-dbg). Call it once the engine reports `bench-hold:`. A
# fixture whose boot opens nothing costs ~1.5 s of retries and no failure.
#   $1  scratch log file
#   $2  `restored` for a session loaded from a save: the meeting the greeting
#       records is saved state, so a restored session greets no one and only
#       the short check runs
smoke_end_boot_conversation() {
    local log="$1" mode="${2:-boot}" attempt attempts=3
    # A fixture whose boot is known to open one (FIXTURE_BOOT_CONVERSATION=1)
    # waits for it: the greeter walks to the player first, so it can open
    # seconds after `bench-hold:`. Ended once, it stays satisfied — Sunny's
    # greeting records the meeting in its End result script.
    [[ "${FIXTURE_BOOT_CONVERSATION:-0}" == 1 && "$mode" != restored ]] && attempts=60
    for ((attempt = 1; attempt <= attempts; attempt++)); do
        debug_command 'dialogue.end' "$log" >/dev/null 2>&1 || true
        if grep -Fq 'dialogue.end: ended' "$log" 2>/dev/null; then
            echo "smoke[${SMOKE_GATE:-gate}]: ended the boot-time conversation (#6052)"
            return 0
        fi
        sleep 0.5
    done
    if [[ "${FIXTURE_BOOT_CONVERSATION:-0}" == 1 && "$mode" != restored ]]; then
        echo "smoke[${SMOKE_GATE:-gate}]: FAIL -- the fixture's boot-time conversation never opened" >&2
        exit 1
    fi
    return 0
}

# Answer every message box the route raised (`ui.answer`), so the simulation
# the box paused resumes. Vanilla tutorials open them on their own — FNV's
# CGTutorial shows "Radiation" the first time the player takes rads — and a
# gate that drives such a state must answer as a player would. Same
# `debug_command <commands> <logfile>` contract as above; a route that raised
# none costs one call.
#   $1  scratch log file
smoke_answer_message_boxes() {
    local log="$1" attempt
    for attempt in 1 2 3 4 5 6; do
        debug_command 'ui.answer' "$log" >/dev/null 2>&1 || true
        if grep -Fq 'no message box is waiting' "$log" 2>/dev/null; then
            return 0
        fi
        if grep -Fq "ui.answer: answered" "$log" 2>/dev/null; then
            echo "smoke[${SMOKE_GATE:-gate}]: answered a message box: $(grep -o "answered '[^']*'" "$log" | head -1)"
        fi
        sleep 0.5
    done
    return 0
}

