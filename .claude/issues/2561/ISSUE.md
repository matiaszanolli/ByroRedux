# FNV-D9-01: guard_system::resolve_anchor duplicates travel_system::resolve_destination's NearReference-FormID-resolve logic instead of sharing it

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2561
**Finding ID**: FNV-D9-01

**Severity**: LOW
**Dimension**: AI Packages & Procedure Runtimes (M42.2–M42.8)
**Location**: `byroredux/src/systems/guard.rs:70-79` (`resolve_anchor`) vs `byroredux/src/systems/travel.rs:81-96` (`resolve_destination`)
**Status**: NEW

## Description
`guard_system::resolve_anchor` duplicates `travel_system::resolve_destination`'s `NearReference`-FormID-resolve logic instead of sharing it, despite that function's own doc comment stating it was generalized specifically so Guard could reuse it (Escort does reuse it, `escort.rs:57`, via an aliased import). `boot.rs:920`'s comment additionally overstates this as "reuses travel_system's anchor-resolution logic," which is true of Escort but not Guard.

## Evidence
Confirmed directly: `guard.rs:70-79`'s `resolve_anchor` re-implements the identical `if let Some(fid) = ... resolve_entity_by_global_form_id ...` pattern that `travel.rs:81-96`'s `resolve_destination` already has as its first half, while `escort.rs:57` imports and calls `travel::resolve_destination` directly (`use super::travel::resolve_destination as resolve_travel_destination;`).

## Impact
Purely a maintainability nit — the two functions are behaviorally correct and intentionally divergent on their fallback today (Guard falls back to `home`, not Travel's hash-pick, by design, previously tried and reverted) — but nothing forces the shared resolve-half to stay in sync if it ever needs a fix.

## Suggested Fix
Extract the shared `NearReference → GlobalTransform.translation` resolve into a small `Option<Vec3>`-returning helper both functions call before applying their own fallback; correct `boot.rs:920`'s wording from "reuses" to "mirrors" for Guard.

## Completeness Checks
- [ ] **TESTS**: Existing Guard/Travel/Escort tests still pass after extracting the shared helper
- [ ] **SIBLING**: Confirm Escort's existing reuse pattern is the correct one to mirror
