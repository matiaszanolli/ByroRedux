# #1441 — LC-D5-02: NIF KeyType::Constant collapsed to Linear

_Snapshot as filed (2026-06-02) from AUDIT_LEGACY_COMPAT_2026-06-02.md. GitHub is authoritative for live state._

- **Severity**: LOW
- **Dimension**: D5 (Animation Readiness)
- **Location**: `byroredux/src/anim_convert.rs:75-76`
- **Status**: NEW

## Description
Core `KeyType` has only `Linear / Quadratic / Tbc` (`crates/core/src/animation/types.rs`). The NIF→core converter maps both `KeyType::XyzRotation => Linear` (`:75`) and `KeyType::Constant => Linear` (`:76`). Gamebryo `KEY_CONST` means *hold value until next key* (step), not interpolate. The NIF-side scalar sampler honors Constant (`keys.rs:130`) but that path only bakes XYZ-Euler axes; the runtime transform sampler never sees a step mode.

## Impact
Transform channels authored with stepped/constant interpolation animate smoothly instead of snapping — wrong motion for hard-cut keyframed scenery / IK poses. Low blast radius (rare in vanilla TRS streams), silently incorrect.

## Suggested Fix
Add a `KeyType::Const` (step) variant to the core enum and have the sampler hold `k0.value` across the segment.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (KF-sequence path vs embedded path; other channel converters)
- [ ] **CANONICAL-BOUNDARY**: per-game logic stays at the NIFAL parser→Material/clip boundary, never pushed to shaders/renderer or re-derived at render time. See /audit-nifal.
- [ ] **TESTS**: Regression test added for this specific fix

_Filed from [docs/audits/AUDIT_LEGACY_COMPAT_2026-06-02.md](../blob/main/docs/audits/AUDIT_LEGACY_COMPAT_2026-06-02.md)_
