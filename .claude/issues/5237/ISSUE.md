# #5237: RT-2026-10-03-03: audit-runtime SKILL.md presents #5118/#4987/#5131/#5133 as open and its gate matrix omits the P4–P6 smokes and m48-5-fnv-hud

**Labels**: documentation, low, tech-debt, doc-rot · **URL**: https://github.com/matiaszanolli/ByroRedux/issues/5237

**Source**: `docs/audits/AUDIT_RUNTIME_2026-10-03.md` RT-3 · **Severity**: LOW · **Dimension**: Harness integrity (doc rot)

**Location**: `.claude/commands/audit-runtime/SKILL.md:20`, `:24`, `:36`, `:108`

## Description
- **:108 (Oblivion neutralisation).** Says `scripts/check-playable-smoke-contracts.sh` does not neutralise `BYROREDUX_OBLIVION_DATA`. `e04fa1ef6` (Fix #5118) now derives the neutralisation list from each fixture's `FIXTURE_DATA_ENV`, and `oblivion.env` declares `BYROREDUX_OBLIVION_DATA`. The script also fails if any harness `*_DATA` variable is left un-neutralised.
- **:24 (#4987).** Calls #4987 "known-open: the lane has not yet been shown to reach a device". It was closed by `6d5d8fa5f` ("make vulkan-validation prove it selected a device").
- **:36 (#5131–#5133).** Says the TSVs carry "still-unattributed" moves of #5131–#5133. `6fcf313e2` attributed #5131 and #5133, and both are closed. Only #5132 (the FO4 spot-light schema row) is open.
- **:20 (gate matrix).**
  - The FO3 `FIXTURE_GATES` example ("p0,p5,p2") omits `p5-f5-f9-quicksave` (the fixture declares `p0-door-interaction p5-save-restart p5-f5-f9-quicksave p2-melee-core`).
  - The playable-slice row omits `p4-quest-route`, `p5-door-transition`, `p5-f5-f9-quicksave`, `p5-quest-persistence`, `p5-soak` and `p6-loading-model`, all of which CLAUDE.md lists as gates.
  - The milestone row omits `m48-5-fnv-hud.sh`.

## Evidence
`grep FIXTURE_DATA_ENV docs/smoke-tests/fixtures/*.env`; `gh issue view 4987` / `5131` / `5133` → CLOSED, `gh issue view 5132` → OPEN; `ls docs/smoke-tests/`.

## Impact
An auditor could file a duplicate for the Oblivion contract, under-run the bless-a-build gate set (the skill says to run the gates you have data for), or treat the vulkan-validation lane as inert.

## Related
#5118, #4987, #5131, #5133, #5132

## Suggested Fix
Delete the :108 bullet. Replace the #4987 parenthetical with a pointer to `6d5d8fa5f`. Rewrite :36 as "#5132 open (spot row)". Extend the :20 gate lists from `docs/smoke-tests/` and the fixtures, then run `.claude/commands/_audit-validate.sh`.

## Completeness Checks
- [ ] **SIBLING**: Other audit skills that quote the smoke-gate list or #4987 (`grep -rn '4987\|FIXTURE_GATES' .claude/commands`) checked for the same staleness
- [ ] **TESTS**: `.claude/commands/_audit-validate.sh` passes with no new advisories
