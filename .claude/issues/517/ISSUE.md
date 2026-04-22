# FNV-ANIM-1: AnimatedColor loses target-slot discriminator

- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/517
- **Severity**: HIGH
- **Dimension**: Animation
- **Audit**: `docs/audits/AUDIT_FNV_2026-04-21.md`
- **Status**: NEW (created 2026-04-21)

## Location

- `crates/core/src/ecs/components/animated.rs:23-28`
- `byroredux/src/systems.rs:334-345`

## Summary

`AnimatedColor(pub Vec3)` has no target discriminator. `animation_system` writes channel value to `AnimatedColor` unconditionally, regardless of whether `channel.target` is Diffuse / Emissive / Specular / ShaderColor. Commit 3ece152's emissive animations render as diffuse tint.

Fix: split into per-target components (`AnimatedDiffuseColor`, `AnimatedEmissiveColor`, etc.) or embed target in the component and branch in the renderer.

Fix with: `/fix-issue 517`
