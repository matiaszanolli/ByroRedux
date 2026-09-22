# GAME-D5-2026-09-21-04: walk_anim module docs still describe fixed-speed 100 u/s movement and never seated and walking

**Issue**: #4710
**Filed**: 2026-09-22 (audit-publish, AUDIT_GAMEPLAY_2026-09-21.md)

**Severity**: LOW
**Dimension**: 5 — doc
**Location**: `byroredux/src/systems/walk_anim.rs`

## Description
Doc still says movement is "the locomotion systems' fixed-speed XZ step... 100 u/s"; since M42.11 (`774560dce`) speed derives from the clip's authored stride. Doc also claims actors are "never both seated and walking", which GAME-D5-2026-09-21-02 (#4703) shows combat breaks.

## Evidence
Doc text verified verbatim; `WalkSpeed` stamped on both spawn paths since `774560dce`.

## Impact
Misleading contract text.

## Related
GAME-D5-2026-09-21-02 (#4703).

## Suggested Fix
Restate both in terms of `WalkSpeed` and the combat chase.
