# FNV-D4-02: Stale/incorrect example FormID in an NCR-faction spot-check test comment

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2558
**Finding ID**: FNV-D4-02

**Severity**: LOW
**Dimension**: ESM Record Parser
**Location**: `crates/plugin/src/esm/records/tests.rs:492-493`
**Status**: NEW

## Description
Stale/incorrect example FormID in an NCR-faction spot-check test comment — cites `0x0011E662` (actually a `REPU` record) instead of the real NCR main faction `0x000A46E7`. The test itself passes correctly via a loose match unaffected by the wrong comment.

## Evidence
Confirmed directly: `tests.rs:492-493` — "Spot-check that NCR faction exists (FNV form 0x0011E662 — name varies by patch; just check there is a faction with 'NCR' in its full name)."

## Impact
Documentation-only; the test logic (a loose name-substring match) is unaffected and correct.

## Suggested Fix
Update the comment to cite `0x000A46E7` (the real NCR main faction FormID), or remove the specific FormID reference since the test doesn't actually depend on it.

## Completeness Checks
- [ ] **TESTS**: N/A (comment-only change); confirm the test's loose-match assertion still passes unchanged
