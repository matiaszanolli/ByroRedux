# SK-D3-01: BSLightingShaderProperty SkinTint/HairTint/EyeEnvmap/etc. colors discarded on import

**Issue**: #343 — https://github.com/matiaszanolli/ByroRedux/issues/343
**Labels**: bug, nif-parser, import-pipeline, high

---

## Severity
**HIGH** — every Skyrim NPC head/body/hair shape uses default specular/albedo with no race-tint multiplier; eyes have no cubemap; multi-layer parallax (ice, frosted glass) renders flat.

## Location
`crates/nif/src/import/material.rs:286-298`

## Description
`extract_material_info` consumes `shader.shader_type_data` only for the `EnvironmentMap` arm (line 295):

```rust
if let ShaderTypeData::EnvironmentMap { env_map_scale } = shader.shader_type_data {
    info.env_map_scale = env_map_scale;
}
```

`SkinTint`, `HairTint`, `Fo76SkinTint`, `EyeEnvmap`, `MultiLayerParallax`, `SparkleSnow`, `ParallaxOcc` all reach this branch, are pattern-matched out, and dropped. There is no field on `MaterialInfo` (lines 60-138) to receive them: no `skin_tint_color`, `hair_tint_color`, `eye_cubemap_scale`, `parallax_max_passes`, `parallax_scale`, `multi_layer_*`, or `sparkle_parameters`.

All 19 BSLightingShaderProperty `shader_type` values are parsed correctly at the byte level — the dispatch in `parse_shader_type_data` is exhaustive — but only EnvironmentMap data ever reaches MaterialInfo.

## Variant Coverage
| # | Variant | Parsed | MaterialInfo |
|---|---|:-:|:-:|
| 0 | Default | ✅ | ✅ |
| 1 | EnvironmentMap | ✅ | partial (env_map_scale only) |
| 2 | GlowShader | ✅ | partial (via common path) |
| 3-18 | Parallax/SkinTint/HairTint/ParallaxOcc/MultiLayerParallax/SparkleSnow/EyeEnvmap/etc. | ✅ | ❌ |

## Impact
Primary visual gap on Skyrim characters and any race-tinted/effect-shaded asset.

## Suggested Fix
Add to `MaterialInfo`:
- `skin_tint_color: Option<[f32; 3]>`
- `hair_tint_color: Option<[f32; 3]>`
- `eye_cubemap_scale: Option<f32>`
- `eye_left_center, eye_right_center: Option<[f32; 3]>`
- `parallax_height_scale, parallax_max_passes: Option<f32>`
- `multi_layer_inner_thickness, multi_layer_refraction_scale, multi_layer_inner_layer_scale, multi_layer_envmap_strength: Option<f32>`
- `sparkle_parameters: Option<[f32; 4]>`

Replace the single-arm `if let` at material.rs:295 with an exhaustive `match shader.shader_type_data { ... }`. Then surface a `material_kind: u32` (mirroring the 19-value enum) on `GpuInstance` so the shader can dispatch (depends on SK-D3-02).

## Completeness Checks
- [ ] **SIBLING**: Verify `extract_material_info_for_bs_tri_shape` (the BsTriShape mirror) gets the same exhaustive match.
- [ ] **TESTS**: Construct synthetic shapes with each shader_type variant; verify MaterialInfo fields populated.
- [ ] **DEPENDS**: Renderer-side `material_kind` dispatch (SK-D3-02) needed for visual effect.

## Related
- Bundles SK-D3-06 (Parallax/ParallaxOcc/MultiLayerParallax height-map params), SK-D3-07 (SparkleSnow params), S4-08 (FO76 refraction_power).

## Source
Audit `docs/audits/AUDIT_SKYRIM_2026-04-16.md` finding **SK-D3-01**.
