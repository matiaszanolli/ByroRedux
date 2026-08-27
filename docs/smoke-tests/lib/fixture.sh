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
    # shellcheck source=/dev/null
    source "$fixture"

    # Per-game data directory: the fixture names its override variable
    # (BYROREDUX_SKYRIM_DATA, BYROREDUX_FNV_DATA, …) and its canonical
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
