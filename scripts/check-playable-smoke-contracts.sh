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

for name in p0-door-interaction p1-character-traversal p2-melee-core; do
    smoke="$ROOT_DIR/docs/smoke-tests/$name.sh"
    for game in "${GAMES[@]}"; do
        set +e
        output="$(BYROREDUX_SKYRIM_DATA="$MISSING_DATA" \
            BYROREDUX_FNV_DATA="$MISSING_DATA" \
            BYROREDUX_FO4_DATA="$MISSING_DATA" "$smoke" "$game" 2>&1)"
        status=$?
        set -e
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
grep -Fq 'input.press: queued action=Attack binding=R' \
    "$ROOT_DIR/docs/smoke-tests/p2-melee-core.sh" \
    || fail "P2 no longer asserts the stable Attack/binding token"
grep -Fq '"grounded=true"' "$ROOT_DIR/docs/smoke-tests/p2-melee-core.sh" \
    || fail "P2 no longer gates floor support"
grep -Fq 'inventory.status' "$ROOT_DIR/docs/smoke-tests/p2-melee-core.sh" \
    || fail "P2 no longer derives damage from the live loadout"
# Post-#3039 these live in the Skyrim fixture rather than inline in P2.
SKYRIM_FIXTURE="$ROOT_DIR/docs/smoke-tests/fixtures/skyrim_se.env"
grep -Fq 'NPC ref=000383F7 base=000E9895' "$SKYRIM_FIXTURE" \
    || fail "the Skyrim fixture no longer pins the grounded reference/base pair"
grep -Fq '0001CB64:DraugrBattleAxe:damage=18' "$SKYRIM_FIXTURE" \
    || fail "the Skyrim fixture no longer pins the Draugr Battleaxe leaf"
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

echo "playable-smoke-contracts: PASS"
