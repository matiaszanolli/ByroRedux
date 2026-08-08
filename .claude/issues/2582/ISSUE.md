# SKY-D2-04: BSEffectShaderProperty.env_map_min_lod is parsed and captured but has no consumer past MaterialInfo -- an undocumented dead-end field

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2582
**Finding ID**: SKY-D2-04

**Severity**: LOW
**Dimension**: BSLightingShaderProperty / BSEffectShaderProperty Shader-Type Dispatch
**Location**: `crates/nif/src/blocks/shader.rs:1560,1736`; `import/material/shader_data.rs:37`; `import/material/mod.rs:894`
**Status**: NEW

## Description
Unlike its packed-field siblings (`texture_clamp_mode` reaches sampler selection, `lighting_influence` reaches `material_flags`), `env_map_min_lod` stops at `BsEffectShaderData` — no packer, no `Material` field, no GLSL uniform — and carries no "parked for a future consumer" comment.

## Evidence
All 8,116 vanilla Skyrim `BSEffectShaderProperty` blocks author `env_map_min_lod = 0`, so nothing is lost today.

## Impact
None on vanilla Skyrim. On FO4+ effect materials that clamp the env-map mip chain, the authored floor is silently ignored.

## Related
#345/S4-01

## Suggested Fix
Either document it as explicitly parked, or plumb it into `GpuMaterial` alongside `soft_falloff_depth`.

## Completeness Checks
- [ ] **TESTS**: N/A unless plumbed to `GpuMaterial`, in which case a regression test confirms the value reaches the shader
