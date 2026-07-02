# FNV-D7-03: Real-data ragdoll test does not pin FNV body/joint counts — a silent joint-drop regression would pass

**Source audit**: `docs/audits/AUDIT_FNV_2026-07-02.md` (finding FNV-D7-03)
**GitHub issue**: https://github.com/matiaszanolli/ByroRedux/issues/1851
**Labels**: low, nif-parser, legacy-compat, tech-debt, bug

**Severity**: LOW
**Dimension**: PHYSAL Ragdoll
**Location**: `crates/nif/tests/ragdoll_import.rs:43-80` (`assert_structural`)
**Status**: NEW

## Description

`docs/engine/physal.md` §7 calls FNV "the measured 18-body / 17-joint reference," but the `#[ignore]` real-data test only asserts structural invariants (`bodies >= 2`, `!constraints.is_empty()`, in-range body indices, `ragdoll_joints + hinge_joints == constraints.len()`). It counts only joints that **survived** decode. A future drop (the FNV-D7-02 breakable path, an `Other`-decode regression, or a field-order slip that fails a `finite` guard) would simply shrink `constraints.len()` and every assertion would still pass.

## Evidence

`assert_eq!(ragdoll_joints + hinge_joints, ragdoll.constraints.len(), ...)` is a self-consistency check on the surviving set, not a completeness check against the 18/17 reference. No `assert!(ragdoll.bodies.len() >= 17)` floor for the FNV arm.

## Impact

The measured reference figure that gives FNV its "reference realization" status is unenforced; joint-loss regressions in the reference slice are invisible to CI (test is `#[ignore]`, but even on manual run it wouldn't catch a drop). Test-coverage gap only; code works today.

## Suggested Fix

Add a game-specific floor for the FNV arm (e.g. `>= 17` bodies, `>= 16` joints, leaving slack for archive/version drift) so a silent joint-loss trips the test on the next manual real-data run.

## Completeness Checks
- [ ] **SIBLING**: Check whether the Oblivion/Skyrim `assert_structural` callers in the same file have (or should have) an equivalent measured-count floor
- [ ] **TESTS**: The new floor assertion itself is the regression pin — verify it fails when a joint is synthetically dropped
