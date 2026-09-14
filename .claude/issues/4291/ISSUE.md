# #4291: REN-2026-09-14-D18-01: ground-cover blades multiply every directional light by `params.x`, which `collect_lights` uploads as 0.0 — grass gets no sun, no traced shadow, no canopy shadow and no translucency

- **Labels**: high,renderer,shaders,terrain-exterior,bug
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4291
- **Filed from**: docs/audits/AUDIT_RENDERER_2026-09-14.md

Source: `docs/audits/AUDIT_RENDERER_2026-09-14.md` (renderer audit, HEAD `147d97c3`)

- **Severity**: HIGH (raised from MEDIUM during orchestrator verification: it removes all direct sun, traced shadow and translucency from every exterior blade — a rendering-correctness defect, which the severity decision tree floors at HIGH)
- **Dimension**: Sky/Weather
- **Location**: `crates/renderer/shaders/groundcover_blade.frag` (`main`, the `incoming` term); `byroredux/src/render/lights.rs` (`collect_lights`)
- **Status**: NEW
- **Description**:
  - The blade pass lights grass only from directional lights (`color_type.w >= 1.5`) and scales each one's radiance by `lights[i].params.x`.
  - `params.x` is the falloff exponent in the `GpuLight` contract (`crates/renderer/src/vulkan/scene_buffer/gpu_types.rs` table; `lighting.glsl` reads it as `falloffShape`).
  - `collect_lights` builds the only directional light in the SSBO, which is both the exterior WTHR sun and the interior XCLL key light. It sets `params: [0.0, 0.0, VisibilityMask::FULL.bits() as f32, …]` and comments that 0.0 is "the 'use default' sentinel so the shader's directional branch ignores it cleanly".
  - As a result `incoming` is exactly zero for the sun every frame. So `lit` and `transmitted` are always zero.
  - The blade's final colour reduces to `albedo * sheenAmbient`, plus composite's `indirect * albedo` from the ground GI left under it (commit `4d54c73a`).
- **Evidence**:
  - `groundcover_blade.frag`: `vec3 incoming = lights[i].color_type.rgb * lights[i].params.x * shadow * canopy;`, followed by `lit += incoming * (diffuse + sheenLobe); transmitted += incoming * (lobe * bladeTransmittance);`.
  - `lights.rs` `collect_lights`: `color_type: [dir_color…, 2.0]`, `params: [0.0, 0.0, VisibilityMask::FULL.bits() as f32, AttenuationModel::LegacySoftRange as u8 as f32]`.
  - No renderer-side pass rewrites `GpuLight.params`. A grep of `crates/renderer/src/vulkan/context/assemble_camera_and_lights.rs` and `crates/renderer/src/vulkan/scene_buffer/upload.rs` found no writer.
  - The canonical directional path does not use the factor. `shadowableLightRadiance` uses `atten = 1.0` and `lightColor * atten`.
  - History: `params.x` has been 0.0 for directionals since `fc338d90` (2026-05-25). The factor has been in the blade shader since Phase 2 (`637b6526`, `lit += … * (wrapped * lights[i].params.x)`) and was carried into #4057's rewrite (`b01ef926`). The contribution has therefore been zero since the shader first shipped.
  - No test pins the factor. A grep for `params.x` in ground-cover tests found none.
- **Impact**:
  - Every exterior ground-cover blade, in every game, is ambient/GI-lit only.
  - Lost with it:
    - the sun's diffuse and sheen lobes;
    - the per-blade traced world shadow (so rocks and trees cast no shadow on grass);
    - the §12.5 canopy transmittance;
    - the §12.2 backlit translucency that #4057 shipped.
  - All of these are computed (including the shadow ray trace on `sceneFlags.x`) and then multiplied by zero, which also wastes one shadow ray per lit blade fragment.
  - The 4d54c73a "sits in the scene's exposure" verification was done in this state, so the §12 lobe calibration has never been observed lit.
- **Related**: #4057 (closed; ground cover light response), `4d54c73a` (GI coupling), D6 (ground cover builds no Material).
- **Suggested Fix**:
  - Drop the `params.x` factor so directional radiance is `color_type.rgb`, matching `shadowableLightRadiance`, and add a shader-source pin against re-multiplying by the falloff lane.
  - Grass brightness will change substantially. Re-check the §12 calibration with a screenshot A/B, not by tuning constants.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types / pipelines / spawn sites)
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **TESTS**: A regression test pins this specific fix
