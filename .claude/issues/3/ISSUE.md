# Issue #3: Non-transform controller channels (material, texture, visibility)

## Metadata
- **Type**: enhancement
- **Severity**: medium
- **Labels**: enhancement, animation, nif-parser, M21
- **State**: OPEN
- **Created**: 2026-04-02
- **Milestone**: M21 follow-up
- **Affected Areas**: NIF animation import, ECS components, animation system

## Problem Statement
KF import only handles `NiTransformController`. Other controller types are silently skipped:
- NiMaterialColorController (diffuse/specular/emissive color)
- NiTextureTransformController (UV offset/scale/rotation)
- NiVisController (visibility toggle)
- NiAlphaController (material alpha)
- BSEffectShader/BSLightingShader controllers

All interpolator/data blocks are already parsed.

## Affected Files
- `crates/nif/src/anim.rs` — extend `import_sequence()` with new channel types
- `crates/core/src/animation.rs` — new channel types (ColorChannel, FloatChannel, BoolChannel)
- `byroredux/src/main.rs` — extend animation_system
- New ECS components: MaterialColor, UvTransform, Visible

## Acceptance Criteria
- [ ] Material color animation imported and applied
- [ ] Texture transform animation imported and applied
- [ ] Visibility animation imported and applied
