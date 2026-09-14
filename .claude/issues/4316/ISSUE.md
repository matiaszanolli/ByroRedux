# #4316: REN-2026-09-14-D21-01: Seven shader-consumed `GpuMaterial` scalars still have no `mat.set` arm, so the soft and rim lighting lobes cannot be exercised from the harness

- **Labels**: low,renderer,test-gap,bug
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4316
- **Filed from**: docs/audits/AUDIT_RENDERER_2026-09-14.md

Source: `docs/audits/AUDIT_RENDERER_2026-09-14.md` (renderer audit, HEAD `147d97c3`)

- **Severity**: LOW
- **Dimension**: Cornell Harness
- **Location**: `byroredux/src/commands/scene.rs` (`MatSetCommand::execute`, its `USAGE` string, the #4023 arm comment)
- **Status**: NEW (#4023's premise was incomplete, not a regression)
- **Description**: #4023 added the four `glass_*` arms under a comment calling them "the last four shader-consumed `GpuMaterial` scalars with no `mat.set` arm". That is false. Seven more scalars are assigned from `Material` into `DrawCommand` in `collect_static_mesh_draws`, carried into `GpuMaterial` by `to_gpu_material`, and read by shaders, yet have no arm and no `USAGE` entry:
  - `lighting_effect_1`
  - `lighting_effect_2`
  - `subsurface_rolloff`
  - `rimlight_power`
  - `backlight_power`
  - `fresnel_power`
  - `grayscale_to_palette_scale`

  The `MAT_FLAG_SOFT_LIGHTING` / `RIM_LIGHTING` / `BACK_LIGHTING` bits can be set through the `material_flags` arm. The scalars those lobes read cannot, and Cornell's constructors leave them at `Material::default()`:
  - **Soft lighting**: `lighting.glsl` takes its wrap width from `subsurfaceRolloff > 0 ? subsurfaceRolloff : lightingEffect1`. Both are 0.0, so the width is 0.
  - **Rim lighting**: the exponent comes from `rimlightPower > 0 ? rimlightPower : lightingEffect2`. The code comment calls both-zero the "nothing authored" state.
  - **Back lighting** falls back to strength 1.0, and `fresnel_power` stays at its default 5.0.

  So the harness can turn the soft and rim lobes on but cannot sweep them away from their degenerate defaults. That is the same shape #2514 (Disney scalars), #2823 (translucency suite) and #4023 (glass optics) each closed one group at a time.
- **Evidence**:
  - The arm list in `MatSetCommand::execute` has no `"lighting_effect_1"`, `"rimlight_power"`, etc., and falls through to `other => Err(format!("unknown field \`{other}\`"))`.
  - `static_meshes.rs`: `rimlight_power: mat.map(|m| m.rimlight_power).unwrap_or(0.0)` and the sibling lines.
  - `rg -l rimlightPower|lightingEffect1|subsurfaceRolloff|fresnelPower|grayscaleToPaletteScale crates/renderer/shaders` hits `crates/renderer/shaders/include/lighting.glsl` and `triangle.frag`.
  - `rg "rimlight|backlight|lighting_effect|fresnel_power|subsurface_rolloff|grayscale_to_palette" byroredux/src/cornell.rs` returns no hits.
- **Impact**: Harness coverage only. The Bethesda soft/rim/back lighting response (landed 2026-08-25, FLT_MAX-sentinel and rim-floor bugs #3452/#3448 in its history) cannot be A/B'd in the RT reference scene, so regressions there have to be found by static reading or on game content.
- **Related**: #4023, #2823, #2514, #3452, #3448, #3460.
- **Suggested Fix**: Add `set_scalar` arms plus `USAGE` entries for the seven fields, and correct the #4023 comment. To stop this recurring a fourth time, add a test asserting every `f32` lane written in `to_gpu_material` either has a `mat.set` arm or appears on an explicit allowlist.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types / pipelines / spawn sites)
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **TESTS**: A regression test pins this specific fix
