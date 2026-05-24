# NIF-DIM4-NEW-01: BSLightingShaderProperty FO4+ PBR scalars parsed but never consumed at import

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/1241

**Source**: `docs/audits/AUDIT_NIF_2026-05-23.md` (Dim 4)
**Severity**: MEDIUM
**Dimension**: Import Pipeline — silent feature drop on Skyrim+ / FO4 / FO76
**Game Affected**: Skyrim / FO4 / FO76

## Description

The Skyrim+ `BSLightingShaderProperty` branch at `crates/nif/src/import/material/walker.rs:291-307` reads `emissive_color`, `emissive_multiple`, `specular_color`, `specular_strength`, `glossiness`, `uv_offset/scale`, and `alpha` into `MaterialInfo`, then delegates to `apply_shader_type_data` for the type-tagged trailer. But seven scalar PBR fields on every BSLightingShaderProperty body — none of them shader-type gated — are captured by the parser at `crates/nif/src/blocks/shader.rs:679-695` and then dropped on the floor:

- `refraction_strength` (`shader.rs:679`)
- `lighting_effect_1` / `lighting_effect_2` (`shader.rs:683-684`) — Skyrim subsurface/backlight scalars (BSVER < FO4) gated by `SLSF2_Soft_Lighting` / `SLSF2_Back_Lighting`
- `subsurface_rolloff` (`shader.rs:687`) — FO4 BSVER 130–139
- `rimlight_power` (`shader.rs:689`) — FO4 BSVER 130–139
- `backlight_power` (`shader.rs:691`) — FO4 BSVER 130–139
- `grayscale_to_palette_scale` (`shader.rs:693`) — FO4+ BSVER ≥ 130
- `fresnel_power` (`shader.rs:695`) — FO4+ BSVER ≥ 130

Each has been the subject of past parse-correctness bugs (#1175 backlight gate inversion, #115 backlight conditional, #403 wetness gate at BSVER 130), confirming the corpus is large enough to matter — yet none reach `MaterialInfo` or `ImportedMesh`, so the import-side investment in parser correctness can't surface in the renderer.

## Evidence

```
$ grep -rn "lighting_effect\|subsurface_rolloff\|rimlight\|backlight_power\|grayscale_to_palette_scale\|fresnel_power\|refraction_strength" crates/nif/src/import/ crates/renderer/src/ byroredux/src/
```
returns zero hits outside test files for these BSLSP-sourced fields. (The `fresnel_power` hits in `byroredux/src/components.rs` + `renderer/src/vulkan/context/draw.rs` are unrelated — they belong to the water/sky fresnel system, not the material PBR ladder.)

The capture branch at `walker.rs:291-307` ends with `apply_shader_type_data` and `has_material_data = true` — none of these seven scalars appear.

## Impact

Skyrim-era subsurface-skin / soft-cloth surfaces render as flat-lit (no SSS approximation, no soft backlight); FO4 power-armor / NPC skin / cloth render without the FO4 rimlight + grayscale-palette PBR contribution; FO76 inherits the same gaps. The renderer's PBR ladder in `triangle.frag` has Fresnel + spec + emissive paths but no fed values for the per-material rolloff/rim/backlight modulators, so the fallback constants apply to every BSLightingShaderProperty surface. Silent feature drop on three games.

## Suggested Fix

Extend `MaterialInfo` (`crates/nif/src/import/material/mod.rs`) with the 7 fields. Copy them at `walker.rs:291-307` from `shader.*` to `info.*` alongside the existing `glossiness` / `alpha` lines. Forward to `ImportedMesh` in all three mesh extractors (`mesh/ni_tri_shape.rs`, `mesh/bs_tri_shape.rs`, `mesh/bs_geometry.rs`) — they already pass through ~30 `mat.*` fields each, so it's a literal-copy expansion.

Renderer consumption can land separately as part of a Skyrim+ PBR pass once BGSM v≥8 path (#1147) is in place — that's the natural pairing site since BGSM provides the same family of scalars for FO4+ external materials.

## Related

- #1175 (CLOSED): backlight gate inversion (parser fix)
- #403 (CLOSED): wetness gate at BSVER 130 (parser fix)
- #1147 (status unknown): BGSM v≥8 translucency suite — same family of PBR scalars from external materials
- #115 (CLOSED): backlight conditional

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: confirm the 7 fields also reach `ImportedMesh` through all three mesh extractor paths (NiTriShape / BSTriShape / BSGeometry), not just `MaterialInfo`. Skipping one extractor would create per-game asymmetry.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: synthetic BSLightingShaderProperty fixture per BSVER band (Skyrim LE/SE = lighting_effect_*; FO4 BSVER 130-139 = subsurface_rolloff + rimlight_power + backlight_power; FO4+ BSVER ≥ 130 = grayscale + fresnel). Assert each lands in `MaterialInfo` and propagates to `ImportedMesh`.