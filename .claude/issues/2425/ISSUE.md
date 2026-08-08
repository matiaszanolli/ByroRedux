# TD7-003: NiControllerSequence's FO3/FNV anim-notes branch uses bare 24..=28 immediately below a sibling branch using the named ANIM_NOTES_THRESHOLD

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2425
**Finding ID**: TD7-003 (source: `docs/audits/AUDIT_TECH-DEBT_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 7 — Magic Numbers & Hardcoded Constants
**Location**: `crates/nif/src/blocks/controller/sequence.rs:305`
**Status**: NEW

## Description
`NiControllerSequence`'s FO3/FNV anim-notes branch uses bare `24..=28` immediately below a sibling branch using the named `bsver::ANIM_NOTES_THRESHOLD`. Two branches on the same `bsver` variable three lines apart — the first correctly uses the named constant (28), the very next `else if` re-hardcodes both bounds. Lower bound (24) has no existing named constant with matching semantics (`bsver::FO3_PARALLAX = 24` is numerically identical but semantically unrelated).

## Related
#2281 (CLOSED, same file, different line — a sibling site the original fix didn't reach), TD7-001, TD7-002.

**Rediscovered (already tracked, not part of this issue):** `shader.rs:1026`'s bare `(130..=139)` FO4-DLC band check is already tracked by open #2343.

**Recurring Pattern:** 6th identifiable wave of "bare `bsver` literal bypasses the named constant module" (#1042 → #1319 → #1336 → #1630 → #2281 → #2343 (open) → this cycle). Two of this cycle's three new sites sit in files with a correctly-named sibling comparison 2-3 lines away — suggesting a habit/review-checklist gap rather than unfamiliarity. Worth a pre-commit grep/lint rather than relying on audit cycles to keep catching it.

## Suggested Fix
Add `bsver::FO3_ANIM_NOTES_LOWER: u32 = 24`, rewrite as a range using both named constants.

## Age
~3.5 months.

## Completeness Checks
- [ ] **TESTS**: Existing NIF controller-sequence parse tests still pass unchanged (no behavior change)
- [ ] **SIBLING**: See TD7-001/TD7-002 — same drift class; consider a pre-commit grep/lint for bare `bsver` literals to stop the recurring wave
