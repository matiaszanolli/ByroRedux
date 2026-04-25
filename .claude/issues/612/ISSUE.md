# SK-D3-04: FO76 SkinTint material_kind=4 never reaches triangle.frag — every FO76 NPC silently skips the SkinTint multiply

## Finding: SK-D3-04

- **Severity**: HIGH
- **Source**: `docs/audits/AUDIT_SKYRIM_2026-04-24.md`
- **Game Affected**: Fallout 76 (NPCs, ghouls, all skin-tinted creatures)
- **Locations**:
  - Parser tag: [crates/nif/src/blocks/shader.rs:639](crates/nif/src/blocks/shader.rs#L639) — `BSShaderType155` enum where SkinTint = 4
  - Importer: [crates/nif/src/import/material.rs:606](crates/nif/src/import/material.rs#L606) — `info.material_kind = shader.shader_type as u8;`
  - Renderer GpuInstance pack: [byroredux/src/render.rs:519-525](byroredux/src/render.rs#L519-L525)
  - Shader gate: [crates/renderer/shaders/triangle.frag:734](crates/renderer/shaders/triangle.frag#L734)

## Description

For `bsver == 155` (FO76), `BSShaderType155` enum has SkinTint as type **4**. The importer copies the raw `shader_type` value into `info.material_kind`. The fragment shader at `triangle.frag:734` only dispatches `materialKind == 5u` (the Skyrim/FO4 SkinTint constant).

The `Fo76SkinTint` payload reaches `skinTintRGBA` in the GpuInstance pack at render.rs:519-525 — but the multiply branch is gated out by the fragment-shader compare. Comment at frag:737 claims FO76 alpha works, but the `== 5u` gate above it locks type 4 out.

Every FO76 NPC, ghoul, and skin-tinted creature renders without their tint multiply.

## Suggested Fix

Two equivalent options:

1. **Remap on import** (1 line): in `material.rs:606`, special-case FO76:
   ```rust
   info.material_kind = if shader.bsver == 155 && shader.shader_type == 4 {
       5  // remap FO76 SkinTint → Skyrim/FO4 constant
   } else {
       shader.shader_type as u8
   };
   ```

2. **Add a sibling shader branch** (preferred, preserves distinct semantics):
   ```glsl
   // triangle.frag near line 734
   if (inst.materialKind == 4u || inst.materialKind == 5u) { /* SkinTint multiply */ }
   ```

Pick (2) if FO76 SkinTint will eventually need a different shader path (e.g. SSS terms). Pick (1) if the multiply is identical.

## Related

- #562 (closed): Skyrim+ BSLightingShaderProperty variant ladder — landed the dispatch but didn't cover the FO76 type-4 vs Skyrim type-5 numbering split.
- #343 (closed): SkinTint colour discarded on import — closed when the Color4 split landed.

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Audit other FO76-vs-legacy enum-numbering splits in `BSShaderType155` (HairTint, EyeEnvmap — verify their material_kind reaches a matching shader branch).
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Add a render-output regression on a vanilla FO76 NPC head — pixels with non-default `skinTintRGBA` should differ from the same head with default.

_Filed from audit `docs/audits/AUDIT_SKYRIM_2026-04-24.md`._
