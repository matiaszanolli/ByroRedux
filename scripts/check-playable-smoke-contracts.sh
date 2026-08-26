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

for name in p0-door-interaction p1-character-traversal p2-melee-core m48-menu-load; do
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
grep -Fq 'NPC ref=000383F7 base=000E9895' \
    "$ROOT_DIR/docs/smoke-tests/p2-melee-core.sh" \
    || fail "P2 no longer pins the grounded reference/base pair"
grep -Fq '0001CB64:DraugrBattleAxe:damage=18' \
    "$ROOT_DIR/docs/smoke-tests/p2-melee-core.sh" \
    || fail "P2 no longer pins the Draugr Battleaxe leaf"
grep -Fq '000236A5:DraugrGreatsword:damage=17' \
    "$ROOT_DIR/docs/smoke-tests/p2-melee-core.sh" \
    || fail "P2 no longer pins the Draugr Greatsword leaf"

# #3273 — M48's gate is only meaningful while it asserts the route's positive
# observable. A gate that merely checks "no error was logged" passes on a run
# where the `--menu` route never executed at all, which is the exact ambiguity
# this smoke test exists to remove.
grep -Fq 'ui.menu: loaded' "$ROOT_DIR/docs/smoke-tests/m48-menu-load.sh" \
    || fail "M48 no longer asserts the menu-loaded success token"
grep -Fq 'Failed to register UI texture' "$ROOT_DIR/docs/smoke-tests/m48-menu-load.sh" \
    || fail "M48 no longer checks the final (texture registration) failure arm"

echo "playable-smoke-contracts: PASS"
