# FNV-ANIM-2: FloatChannel UV and Morph targets never applied

- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/525
- **Severity**: MEDIUM
- **Dimension**: Animation
- **Audit**: `docs/audits/AUDIT_FNV_2026-04-21.md`
- **Status**: NEW (created 2026-04-21)

## Location

`byroredux/src/systems.rs:316-330, 559-568`

## Summary

Float-channel dispatch only honors `FloatTarget::Alpha`. UvOffsetU/V, UvScaleU/V, UvRotation, ShaderFloat, MorphWeight all sampled correctly but dropped — no ECS sink. Animated UV scrolling (water, lava, HUD) and FaceGen lip-sync silently do nothing.

Fix: add `AnimatedUvTransform`, `AnimatedMorphWeights`, `AnimatedShaderFloat` sparse components; route each `FloatTarget` variant.

Fix with: `/fix-issue 525`
