# FNV-ANIM-3: Root-motion split comment conflates Gamebryo-XY with renderer-XZ

- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/526
- **Severity**: MEDIUM (doc / downstream mis-use)
- **Dimension**: Animation
- **Audit**: `docs/audits/AUDIT_FNV_2026-04-21.md`
- **Status**: NEW (created 2026-04-21)

## Location

`crates/core/src/animation/root_motion.rs:23-27`

## Summary

Math is correct (post Z-up→Y-up conversion), but the "horizontal (XZ)" comment misleads: downstream consumers reading `RootMotionDelta` as legacy Gamebryo-XY will be off by 90°. Character controllers will ship with wrong-axis motion.

Fix: clarify comment ("horizontal in renderer Y-up space"); add regression test for known-input walking animation.

Fix with: `/fix-issue 526`
