# GAME-D7-2026-09-21-02: The save-registry completeness guard reads only up to each file's first #[cfg(test)] — 39 production types are invisible, 5 unclassified

**Issue**: #4705
**Filed**: 2026-09-22 (audit-publish, AUDIT_GAMEPLAY_2026-09-21.md)

**Severity**: MEDIUM
**Dimension**: 7 — State Coverage
**Location**: `byroredux/src/save_io/registry_completeness_tests.rs` (`src.split("#[cfg(test)]").next()`); `byroredux/src/components.rs` (mid-file `#[cfg(test)] mod region_ambient_res_tests`)

## Description
The guard truncates each production file at the first `#[cfg(test)]`, assuming test modules sit at file tails. Several files (`components.rs`, `load_order.rs`, `skinned_mesh.rs`, `save_io.rs`, `extensions/systems.rs`) gate an item mid-file, hiding everything declared after it from classification.

## Evidence
`/tmp/audit/gameplay/hidden_types.py` (brace-matched) finds 39 hidden production impls; 5 neither registered nor allowlisted: `DraugrCombatAnim`, `DraugrCombatClips`, `ExposureTuning`, `NavmeshResidency` (all `components.rs`), `GlobalFormIdResolver` (`cell_loader/load_order.rs`).

## Impact
The guard is green while blind to 39 types — exactly how `DraugrCombatAnim`'s `death_played` latch (GAME-D4-2026-09-21-05) went unexamined for save correctness.

## Related
#2295 / #3166 / #3497 (guard history).

## Suggested Fix
Strip `#[cfg(test)]` items by brace-matching instead of a first-occurrence split. Classify the five surfaced types. Add a guard self-test with a mid-file test module.
