# CHAR-2026-08-16-D6-01: CHARAL builder tests cannot falsify the roster

**Issue**: #3095
**Severity**: MEDIUM
**Labels**: `medium,tech-debt,bug`
**Source report**: `docs/audits/AUDIT_CHARACTER_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_CHARACTER_2026-08-16.md` (Dimension 6 — test fidelity).

**Location**: `crates/core/src/character/fallout.rs`:162-181 (`full`) and the sibling builder tests

## Description

**Every CHARAL builder test supplies a resolver built from the roster's own strings** — so the fixtures cannot falsify the roster.

The test resolver is constructed by mapping each roster entry to a synthetic id. Any roster entry therefore resolves by construction, regardless of whether a real `AVIF` with that EditorID exists in any shipped game.

## Impact

This is the green-by-construction mechanism that let **#2986, #3093 and #3094 all survive**: three independent roster/EditorID defects, none detectable by any existing CHARAL test, because the tests validate the roster against itself.

The builder tests are otherwise thorough — the defect is specifically that their input is derived from their subject.

## Suggested Fix

Add at least one real-data test (`#[ignore]`d, per the house pattern for data-dependent tests) that builds each ruleset with a resolver backed by the shipped master's actual `AVIF` table, and asserts the resulting roster is non-empty and complete.

Keep the synthetic tests for logic coverage — the gap is that nothing checks the roster against reality.

## Related

- **#2986, #3093, #3094 — the three roster defects this test shape could not catch**
- #3014, #3017, #3083 (the same green-by-construction class elsewhere this sweep)

## Completeness Checks
- [ ] **REAL-RESOLVER**: At least one test resolves against a shipped master's `AVIF` table
- [ ] **FALSIFIABLE**: A deliberately wrong roster entry fails the new test
- [ ] **ALL-RULESETS**: Each per-game ruleset gets the treatment, not only FO3/FNV
- [ ] **HOUSE-PATTERN**: The data-dependent test is `#[ignore]`d and its invocation documented
- [ ] **TESTS**: The new test fails before #2986/#3094 and passes after

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3095 --json state` when live state is needed.*
