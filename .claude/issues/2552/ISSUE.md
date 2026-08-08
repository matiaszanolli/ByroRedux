# FO3-D5-NEW-04: spawn_collision_shapes's catch_unwind guards a Clone that can't panic; stale comment

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2552
**Finding ID**: FO3-D5-NEW-04

**Severity**: LOW
**Dimension**: FO3 Collision Import (Havok → CollisionShape)
**Location**: `byroredux/src/cell_loader/spawn.rs:956-965`
**Status**: NEW

## Description
`spawn_collision_shapes`'s `catch_unwind` wraps `coll.shape.clone()` with the comment "parry3d panics on nested Compound shapes. Clone inside catch_unwind so a bad shape doesn't kill the entire load." But `coll.shape` is a canonical `CollisionShape` enum — a plain Rust data structure — and `.clone()` on it cannot panic regardless of shape nesting; it's a pure data copy. `#373` restructured the physics conversion (`crates/physics/src/convert.rs`) to depth-first-flatten any Compound-of-Compound into a `Vec<(Isometry3, SharedShape)>` specifically so parry3d/Rapier never sees a nested `SharedShape::compound` — the panic condition this comment describes was the *old*, pre-#373 conversion shape, not anything `Clone` could ever trigger.

## Evidence
Confirmed directly: `spawn.rs:956-965` — the comment describes a parry3d panic risk, but the guarded expression is `std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| coll.shape.clone()))`, a `Clone` call with no parry3d/Rapier involvement. `convert.rs`'s own module doc confirms the flattening approach: "Parry / Rapier forbid composite-inside-compound... Returning a `Vec<(Isometry3, SharedShape)>` instead of a single `SharedShape::compound`... See #373."

## Impact
None on running code — the `catch_unwind` is harmless dead-weight (guards a call that structurally cannot panic) and the warning log path is unreachable. Hygiene/documentation issue: a future reader trying to understand the panic risk will chase a mechanism (nested-Compound parry3d panic) that no longer exists at this call site.

## Related
`#373` (the flattening fix that removed the actual constraint this comment describes).

## Suggested Fix
Remove the now-unnecessary `catch_unwind`/`AssertUnwindSafe` wrapper around the plain `.clone()` call, or if retained defensively, correct the comment to state it's a legacy guard with no known trigger post-#373.

## Completeness Checks
- [ ] **TESTS**: If the `catch_unwind` is removed, confirm no test relies on its warning-log fallback path
