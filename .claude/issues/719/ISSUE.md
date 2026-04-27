# NIF-D4-03: BSEffectShaderProperty FO4+ env_map / env_mask textures never reach MaterialInfo.env_map

URL: https://github.com/matiaszanolli/ByroRedux/issues/719
Labels: bug, nif-parser, import-pipeline, medium

---

## Severity: MEDIUM

## Game Affected
FO4, FO76, Starfield

## Location
- `crates/nif/src/import/material/walker.rs:158-263` (BSEffectShaderProperty branch in `extract_material_info_from_refs`)

## Description
For BSEffectShaderProperty meshes on FO4+ the parser populates `shader.env_map_texture` and `shader.env_mask_texture` (BSVER >= 130 fields, `shader.rs:1102-1108`). The importer captures these into `info.effect_shader.env_map_texture` / `env_mask_texture` via `capture_effect_shader_data` (line 218).

However the `MaterialInfo.env_map` / `env_mask` slots — which are what the renderer routes through the GPU instance for the standard env-map shading branch — are only populated from BSShaderTextureSet (Skyrim+ BSLightingShaderProperty path at lines 119-127) or BSShaderPPLightingProperty's texture set. The `info.normal_map` slot DOES get fed from `shader.normal_texture` (line 194-195) but `env_map` / `env_mask` are not similarly forwarded.

Renderer-side dispatch still checks `mat.env_map`, so FO4+ effect-shader env reflections (force-field reflections, Dwemer steam, magic shields) bind no env texture even when one is authored.

## Evidence
```
walker.rs:194:                if !shader.normal_texture.is_empty() {
walker.rs:195:                    info.normal_map = Some(shader.normal_texture.clone());
walker.rs:196:                }
walker.rs:197:                info.env_map_scale = shader.env_map_scale;
```
No corresponding `info.env_map = Some(shader.env_map_texture.clone())` branch. Verified at HEAD `09dbcfc`.

## Impact
FO4+ shielded / energy / chrome effect surfaces lose their env-map reflection. Visible regression on Power Armor frames, energy weapons, and force-field VFX. Verified by `effect_shader` field carrying the textures but `env_map` field staying `None`.

## Suggested Fix
After `info.env_map_scale = shader.env_map_scale;` (walker.rs:197) add:
```rust
if !shader.env_map_texture.is_empty() && info.env_map.is_none() {
    info.env_map = Some(shader.env_map_texture.clone());
}
if !shader.env_mask_texture.is_empty() && info.env_mask.is_none() {
    info.env_mask = Some(shader.env_mask_texture.clone());
}
```
Both are already routed through `is_empty()` filters in `capture_effect_shader_data`.

## Related
- Audit: docs/audits/AUDIT_NIF_2026-04-26.md (NIF-D4-03)
- Adjacent: #620 (BSEffectShaderProperty falloff fields not on GPU — different fields, same shader type)

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Verify the BSLightingShaderProperty branch (lines 119-127) doesn't have the same gap for any FO4+ env fields
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Regression test that an FO4 BSEffectShader with `env_map_texture = "foo.dds"` produces `MaterialInfo { env_map: Some("foo.dds") }`
