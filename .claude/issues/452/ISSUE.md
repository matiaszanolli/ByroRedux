# Issue #452

FO3-REN-M1: BSShaderTextureSet slots 3/4/5 (parallax/env/mask) never read on PPLighting path

---

## Severity: Medium

**Location**: `crates/nif/src/import/material.rs:617-651`

## Problem

Importer reads slots 0 (base), 1 (normal), 2 (glow) on the `BSShaderPPLightingProperty` branch. Slots 3 (parallax/height), 4 (env cube), 5 (env mask) are never pulled into `MaterialInfo`.

`parallax_max_passes` / `parallax_height_scale` are captured only from `ShaderTypeData::ParallaxOcc` (BSLightingShaderProperty — Skyrim+), never from FO3 PPLighting shader_type codes:
- `shader_type=3` (Parallax_Shader_Index_15, flag1 bit 11 / 0x800)
- `shader_type=7` (Parallax_Occlusion, flag1 bit 28 / 0x10000000)

## Impact

- Pitt/Point Lookout parallax brick walls render flat.
- FNV Hoover Dam concrete renders flat.
- Glass bottle / power-armor env reflections never bind — `GpuMaterial.env_map_scale` is captured but has no texture route.

## Fix

Extend PPLighting branch (line 617-651):
```rust
if let Some(parallax) = tex_set.textures.get(3).filter(|s| !s.is_empty()) {
    info.parallax_map = Some(parallax.clone());
}
if let Some(env) = tex_set.textures.get(4).filter(|s| !s.is_empty()) {
    info.env_map = Some(env.clone());
}
// slot 5 = env mask — map to info.env_mask
```

Capture `parallax_max_passes` + `parallax_scale` from `BSShaderPPLightingProperty` directly (they're already parsed into the block at `shader.rs:83-84`).

Related to but distinct from #353 (Skyrim env slot — BSLightingShaderProperty path).

## Completeness Checks

- [ ] **TESTS**: Synthetic PPLighting block with slot 3/4/5 populated → `MaterialInfo` carries paths
- [ ] **SIBLING**: Downstream — issue FO3-REN-M2 tracks the `GpuInstance`/shader-side binding (new issue)
- [ ] **SIBLING**: Verify Oblivion NiTexturingProperty parallax slot (FO3-NIF-M1) lands in the same downstream consumer

Audit: `docs/audits/AUDIT_FO3_2026-04-19.md` (FO3-REN-M1)
