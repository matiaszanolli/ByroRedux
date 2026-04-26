# SK-D4-01: BSEffectShaderProperty falloff fields captured in MaterialInfo but never reach GPU — magic VFX have hard alpha edges

## Finding: SK-D4-01

- **Severity**: MEDIUM
- **Source**: `docs/audits/AUDIT_SKYRIM_2026-04-24.md`
- **Game Affected**: Skyrim, FO4, FO76 (magic VFX, decals, smoke planes, water edges)
- **Locations**:
  - Capture site: [crates/nif/src/import/material.rs:438-441](crates/nif/src/import/material.rs#L438-L441) (populates `BsEffectShaderData`)
  - Type def: [crates/nif/src/import/material.rs:331-340](crates/nif/src/import/material.rs#L331-L340) (`falloff_start_angle`, `falloff_stop_angle`, `falloff_start_opacity`, `falloff_stop_opacity`, `soft_falloff_depth`)
  - GPU plumbing: no reference to `falloff_*` or `soft_falloff_depth` in [byroredux/src/render.rs](byroredux/src/render.rs) or [crates/renderer/src/mesh.rs](crates/renderer/src/mesh.rs)

## Description

`BSEffectShaderProperty.{falloff_start_angle, falloff_stop_angle, falloff_start_opacity, falloff_stop_opacity, soft_falloff_depth}` are fully parsed and populate `BsEffectShaderData` end-to-end through the importer (`capture_effect_shader_data` at material.rs:430).

Searching for `falloff_*` or `soft_falloff_depth` in `byroredux/src/render.rs` and `crates/renderer/src/mesh.rs` returns zero hits. The data never reaches a `GpuInstance` field or the fragment shader.

Effect-shader meshes (magic VFX, decal planes, smoke quads, water edges) render with hard alpha edges instead of view-angle falloff or soft-depth feathering against the background.

Distinct from #354 (closed: BSEffectShaderProperty alpha not exposed — that fixed the alpha-blend pipeline selection). This is the falloff fields specifically.

## Suggested Fix

1. Extend `GpuInstance` with the 5-field falloff struct (under a feature flag if size budget is tight; pack tightly: 5 × f32 = 20 bytes plus alignment).
2. In `byroredux/src/render.rs`, when material is BSEffectShaderProperty, copy `bs_effect.falloff_*` and `bs_effect.soft_falloff_depth` into the new GpuInstance fields.
3. In `triangle.frag`, when `materialKind` indicates effect-shader, compute:
   - View-angle falloff: `t = saturate((dot(N, V) - cos(stop_angle)) / (cos(start_angle) - cos(stop_angle)))` then `lerp(stop_opacity, start_opacity, t)` — multiply into final alpha.
   - Soft-depth: compare fragment depth against `gbuffer_depth` (already available in composite pass); fade alpha by `saturate((scene_depth - frag_depth) / soft_falloff_depth)`.

Both are standard effect-shader formulas — match Bethesda's reference behaviour as closely as the captured fields allow.

## Related

- #354 (closed): BSEffectShaderProperty alpha-blend pipeline selection.
- #434 (closed): BsTriShape drops BSEffectShaderProperty .bgem material_path — adjacent.

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: BSShaderNoLightingProperty has a similar falloff_cone (#451). Verify that path correctly reaches the shader.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Render-output diff on a known magic VFX mesh — pixels at high `dot(N,V)` should differ between falloff-on and falloff-off.

_Filed from audit `docs/audits/AUDIT_SKYRIM_2026-04-24.md`._
