#!/usr/bin/env bash
# CI-safe contract checks for the real-data playable-slice smoke gates.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MISSING_DATA="$(mktemp -d /tmp/byro-missing-skyrim-data.XXXXXX)"
trap 'rm -rf "$MISSING_DATA"' EXIT

fail() {
    echo "playable-smoke-contracts: FAIL -- $*" >&2
    exit 1
}

# #3039 — the three playable-slice gates are game-parameterised. Every
# shipped fixture must honour the SKIP != PASS contract, not just the
# default game, or a title could be "covered" by a gate that silently
# no-ops on the runner.
mapfile -t GAMES < <(cd "$ROOT_DIR/docs/smoke-tests/fixtures" && ls ./*.env | sed 's|^\./||; s|\.env$||')
(( ${#GAMES[@]} >= 2 )) \
    || fail "expected at least the skyrim_se and fnv fixtures, found ${#GAMES[@]}"
echo "playable-smoke-contracts: fixtures = ${GAMES[*]}"

for name in p0-door-interaction p1-character-traversal p2-melee-core p5-save-restart w1-water-traversal; do
    smoke="$ROOT_DIR/docs/smoke-tests/$name.sh"
    for game in "${GAMES[@]}"; do
        unset FIXTURE_GATES
        source "$ROOT_DIR/docs/smoke-tests/fixtures/$game.env"
        supported=1
        if declare -p FIXTURE_GATES &>/dev/null; then
            supported=0
            for gate in "${FIXTURE_GATES[@]}"; do
                [[ "$gate" == "$name" ]] && supported=1
            done
        fi
        set +e
        output="$(BYROREDUX_SKYRIM_DATA="$MISSING_DATA" \
            BYROREDUX_FNV_DATA="$MISSING_DATA" \
            BYROREDUX_FO3_DATA="$MISSING_DATA" \
            BYROREDUX_FO4_DATA="$MISSING_DATA" "$smoke" "$game" 2>&1)"
        status=$?
        set -e
        if (( supported == 0 )); then
            [[ $status -eq 2 ]] \
                || fail "$name[$game] unmeasured route exited $status instead of configuration error=2"
            grep -Fq 'has no measured route for this gate' <<<"$output" \
                || fail "$name[$game] did not explain its unmeasured route"
            echo "playable-smoke-contracts: PASS -- $name[$game] rejects unmeasured routes explicitly"
            continue
        fi
        [[ $status -eq 77 ]] \
            || fail "$name[$game] missing-data path exited $status instead of SKIP=77"
        grep -Fq "smoke[$name]: SKIP -- missing" <<<"$output" \
            || fail "$name[$game] did not emit an explicit SKIP diagnostic"
        echo "playable-smoke-contracts: PASS -- $name[$game] distinguishes SKIP from PASS"
    done
done

for name in m48-menu-load; do
    smoke="$ROOT_DIR/docs/smoke-tests/$name.sh"
    set +e
    output="$(BYROREDUX_SKYRIM_DATA="$MISSING_DATA" BYROREDUX_FO4_DATA="$MISSING_DATA" "$smoke" 2>&1)"
    status=$?
    set -e
    [[ $status -eq 77 ]] \
        || fail "$name missing-data path exited $status instead of SKIP=77"
    grep -Fq "smoke[$name]: SKIP -- missing" <<<"$output" \
        || fail "$name did not emit an explicit SKIP diagnostic"
    echo "playable-smoke-contracts: PASS -- $name distinguishes SKIP from PASS"
done

# An unknown game must be a loud configuration error, never a SKIP that a CI
# lane would read as "data absent, nothing to do".
set +e
output="$("$ROOT_DIR/docs/smoke-tests/p0-door-interaction.sh" no-such-game 2>&1)"
status=$?
set -e
[[ $status -eq 2 ]] || fail "an unknown game exited $status instead of 2"
grep -Fq "no fixture for game 'no-such-game'" <<<"$output" \
    || fail "an unknown game did not name the missing fixture"
echo "playable-smoke-contracts: PASS -- an unknown game fails loudly, not as SKIP"

grep -Fq 'input.press: queued action=Activate binding=E' \
    "$ROOT_DIR/docs/smoke-tests/p0-door-interaction.sh" \
    || fail "P0 no longer asserts the stable Activate/binding token"
grep -Fq 'input.press: queued action=Activate binding=E' \
    "$ROOT_DIR/docs/smoke-tests/p1-character-traversal.sh" \
    || fail "P1 no longer asserts the stable Activate/binding token"
# The CLI deliberately accepts space-separated values only. An equals sign
# silently exercises default startup placement instead of the fixture pose
# when the smoke's log filter hides the parser warning.
grep -Fq -- '--camera-pos "$P1_CAMERA_POS"' \
    "$ROOT_DIR/docs/smoke-tests/p1-character-traversal.sh" \
    || fail "P1 no longer passes its camera position using supported CLI syntax"
grep -Fq -- '--camera-forward "$P1_CAMERA_FORWARD"' \
    "$ROOT_DIR/docs/smoke-tests/p1-character-traversal.sh" \
    || fail "P1 no longer passes its camera direction using supported CLI syntax"
grep -Fq 'input.press: queued action=Attack binding=R' \
    "$ROOT_DIR/docs/smoke-tests/p2-melee-core.sh" \
    || fail "P2 no longer asserts the stable Attack/binding token"
grep -Fq '"grounded=true"' "$ROOT_DIR/docs/smoke-tests/p2-melee-core.sh" \
    || fail "P2 no longer gates floor support"
grep -Fq 'inventory.status' "$ROOT_DIR/docs/smoke-tests/p2-melee-core.sh" \
    || fail "P2 no longer derives damage from the live loadout"
grep -Fq 'save load: restored player pose' "$ROOT_DIR/docs/smoke-tests/p2-melee-core.sh" \
    || fail "P2 no longer waits for a completed save restore"
grep -Fq 'cond $restored_target GetDead' "$ROOT_DIR/docs/smoke-tests/p2-melee-core.sh" \
    || fail "P2 no longer checks death persistence on the remapped reference"
grep -Fq '"cooldown_ready=true"' "$ROOT_DIR/docs/smoke-tests/p2-melee-core.sh" \
    || fail "P2 no longer waits for exact cooldown readiness"
grep -Fq 'ragdoll.status $restored_target' "$ROOT_DIR/docs/smoke-tests/p2-melee-core.sh" \
    || fail "P2 no longer samples restored corpse physics"
grep -Fq 'live ragdoll has missing bodies or non-finite physics before saving' \
    "$ROOT_DIR/docs/smoke-tests/p2-melee-core.sh" \
    || fail "P2 no longer distinguishes live corpse corruption from restore corruption"
grep -Fq 'complete=true finite=true' "$ROOT_DIR/docs/smoke-tests/p2-melee-core.sh" \
    || fail "P2 no longer rejects incomplete or non-finite corpse physics"

# WATAL W1 — the water gate's whole value is the four transitions it pins. A
# gate that only proved "the player got wet" would pass on the pre-W1 engine,
# where a dive stalled at the buoyancy spring's neutral point, an upward swim
# could launch the capsule out of the lake, and the exit frame inherited the
# spring as terrestrial momentum.
W1_GATE="$ROOT_DIR/docs/smoke-tests/w1-water-traversal.sh"
grep -Fq 'a swimmer must not report ground contact' "$W1_GATE" \
    || fail "W1 no longer gates the swimming/grounded exclusion"
grep -Fq 'the kinematic player did not reach the canonical WaterContact sink' "$W1_GATE" \
    || fail "W1 no longer gates the player's canonical WaterContact publish"
grep -Fq 'the swimmer left the water by swimming upward (surface clamp lost)' "$W1_GATE" \
    || fail "W1 no longer gates the surface clamp"
grep -Fq 'buoyancy spring leaked into gravity' "$W1_GATE" \
    || fail "W1 no longer gates the water-exit velocity reset"
w1_profiles=0
for fixture in "$ROOT_DIR"/docs/smoke-tests/fixtures/*.env; do
    game="$(basename "$fixture" .env)"
    # A title without a measured W1 route declares none and the gate SKIPs it
    # (77) — that is the SKIP != PASS contract applied to fixtures. A fixture
    # that *does* declare one must pin the authored WATR form it traverses,
    # and must state explicitly whether its water can submerge a head rather
    # than leaving the script to infer it.
    if ! grep -q '^W1_WATER_SOURCE=' "$fixture"; then
        echo "playable-smoke-contracts: PASS -- $game declares no W1 route (not counted as coverage)"
        continue
    fi
    w1_profiles=$(( w1_profiles + 1 ))
    grep -Eq '^W1_WATER_SOURCE="0x[0-9A-F]{8}"$' "$fixture" \
        || fail "fixture $game does not pin a W1 WATR source form"
    grep -Eq '^W1_HEAD_SUBMERSION=[01]$' "$fixture" \
        || grep -Fq 'W1_DIVE_DEPTH' "$fixture" \
        || fail "fixture $game declares neither head submersion nor a dive depth"
    grep -Eq '^W1_BOUNDARY_PHASE=(after-exit|before-exit)$' "$fixture" \
        || grep -Fvq 'W1_BOUNDARY_ACTION' "$fixture" \
        || fail "fixture $game declares a W1 boundary leg without its phase"
done
(( w1_profiles >= 1 )) \
    || fail "no fixture declares a W1 water route — the traversal gate would never run"
# Post-#3039 these live in the Skyrim fixture rather than inline in P2.
SKYRIM_FIXTURE="$ROOT_DIR/docs/smoke-tests/fixtures/skyrim_se.env"
grep -Fq 'NPC ref=000383F7 base=000E9895' "$SKYRIM_FIXTURE" \
    || fail "the Skyrim fixture no longer pins the grounded reference/base pair"
grep -Fq '0002C672:DraugrWarAxe:damage=9' "$SKYRIM_FIXTURE" \
    || fail "the Skyrim fixture no longer pins the reachable Draugr War Axe leaf"
grep -Fq '000236A5:DraugrGreatsword:damage=17' "$SKYRIM_FIXTURE" \
    || fail "the Skyrim fixture no longer pins the Draugr Greatsword leaf"

# #3039 FNV — the reference title must keep a gate of its own. A fixture
# that quietly drops back to Skyrim's cell would re-open the exact gap
# FNV-2026-08-16-D8-01 reported.
FNV_FIXTURE="$ROOT_DIR/docs/smoke-tests/fixtures/fnv.env"
grep -Fq 'FIXTURE_ESM="FalloutNV.esm"' "$FNV_FIXTURE" \
    || fail "the FNV fixture no longer targets FalloutNV.esm"
grep -Fq 'NPC ref=00104C6D base=00104C6C' "$FNV_FIXTURE" \
    || fail "the FNV fixture no longer pins its frozen reference/base pair"

# #3273 — M48's gate is only meaningful while it asserts the route's positive
# observable. A gate that merely checks "no error was logged" passes on a run
# where the `--menu` route never executed at all, which is the exact ambiguity
# this smoke test exists to remove.
grep -Fq 'ui.menu: loaded' "$ROOT_DIR/docs/smoke-tests/m48-menu-load.sh" \
    || fail "M48 no longer asserts the menu-loaded success token"
grep -Fq 'Failed to register UI texture' "$ROOT_DIR/docs/smoke-tests/m48-menu-load.sh" \
    || fail "M48 no longer checks the final (texture registration) failure arm"

# #3160 — M47's recognition gate was SOFT, so deleting `attach_vmad_scripts`
# outright left the domain's only engine-side gate on real game data green.
# It is now HARD on the pinned default cell with the script archive
# resolved. Run the harness's own decision-table self-test (no engine, no
# game data) so the promotion cannot be silently reverted, and so this
# check does not merely assert that some text is present.
M47_SMOKE="$ROOT_DIR/docs/smoke-tests/m47-triggers.sh"
bash "$M47_SMOKE" --self-test > /dev/null \
    || fail "m47-triggers.sh --self-test failed (recognition gate decision table)"
# The self-test must keep covering the regression case itself — a decision
# table that dropped the FAIL row would pass while proving nothing.
grep -Fq 'check FAIL 0 1 1' "$M47_SMOKE" \
    || fail "m47-triggers.sh --self-test no longer covers the zero-recognized HARD-fail case"
grep -Fq 'hard_fail=1' "$M47_SMOKE" \
    || fail "m47-triggers.sh no longer has a HARD failure path for zero recognized REFRs"

# EXT-D7-2026-09-21-01 / #4730 — every gate the workflow dispatches must
# resolve to a real script. The m-exteriors arm passed `m-exteriors.sh`
# into run_gate, which appends `.sh` itself, so CI executed
# `docs/smoke-tests/m-exteriors.sh.sh` (status 127) and the whole exterior
# matrix measured nothing. Guard both the literal run_gate call sites and
# the workflow's declared `gate` options that route through the dynamic
# `*` arm. Comment text is stripped first so prose mentioning run_gate
# cannot satisfy — or break — the scan.
WORKFLOW="$ROOT_DIR/.github/workflows/playable-smoke.yml"
mapfile -t LITERAL_GATES < <(
    sed 's/#.*//' "$WORKFLOW" \
        | grep -oE 'run_(declared_)?gate [a-zA-Z0-9-]+' \
        | sed -E 's/run_(declared_)?gate //' | sort -u
)
(( ${#LITERAL_GATES[@]} >= 1 )) \
    || fail "playable-smoke.yml declares no literal run_gate calls — did the dispatch shape change?"
for gate in "${LITERAL_GATES[@]}"; do
    [[ -f "$ROOT_DIR/docs/smoke-tests/$gate.sh" ]] \
        || fail "playable-smoke.yml calls run_gate $gate but docs/smoke-tests/$gate.sh does not exist (run_gate appends .sh itself)"
done
echo "playable-smoke-contracts: PASS -- literal run_gate calls resolve to scripts (${LITERAL_GATES[*]})"

mapfile -t GATE_OPTIONS < <(
    sed -n '/^      gate:/,/^jobs:/p' "$WORKFLOW" \
        | grep -E '^[[:space:]]+- ' | sed 's/^[[:space:]]*- //'
)
(( ${#GATE_OPTIONS[@]} >= 1 )) \
    || fail "playable-smoke.yml declares no gate options — did the input shape change?"
for gate in "${GATE_OPTIONS[@]}"; do
    case "$gate" in
        # `all` is the fixture-declared bundle; the other two are dedicated
        # arms that never hand a script name to run_gate.
        all|m-exteriors-static|groundcover-eval) continue ;;
    esac
    [[ -f "$ROOT_DIR/docs/smoke-tests/$gate.sh" ]] \
        || fail "playable-smoke.yml offers gate option $gate but docs/smoke-tests/$gate.sh does not exist"
done
echo "playable-smoke-contracts: PASS -- workflow gate options resolve to scripts"

echo "playable-smoke-contracts: PASS"
