# Issue #977

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/977
**Title**: NIF-D4-NEW-03: BSSkyShaderProperty / BSWaterShaderProperty parsed but never consumed — Skyrim sky meshes render as magenta
**Labels**: bug, nif-parser, renderer, import-pipeline, medium
**Audit source**: docs/audits/AUDIT_NIF_2026-05-12.md

---

**Source**: `docs/audits/AUDIT_NIF_2026-05-12.md` (Dim 4)
**Severity**: MEDIUM
**Dimension**: Import Pipeline
**Game Affected**: Skyrim SE, FO4, FO76
**Location**: parsed at `crates/nif/src/blocks/shader.rs:493-599`; no `scene.get_as::<BSSkyShaderProperty>` / `<BSWaterShaderProperty>` site exists in `crates/nif/src/import/material/walker.rs` (confirmed via `grep -rn 'BSSkyShaderProperty\|BSWaterShaderProperty' crates/nif/src/import/` — zero hits)

## Description

Both Skyrim-era subclasses are fully parsed:

- `BSSkyShaderProperty.source_texture: String` + `sky_object_type: u32` (clouds, sun glare, moon, stars)
- `BSWaterShaderProperty.uv_offset / uv_scale` + `water_shader_flags: u32` (reflections, refractions, cubemap, fog)

But neither has any importer consumer. Every Skyrim `meshes/sky/*.nif` (clouds, sunglare, moon, stars) binds these via `shader_property_ref` — they all import with `texture_path = None` and render as the magenta placeholder.

The FO3/FNV-era counterparts (`SkyShaderProperty`, `TallGrassShaderProperty`, `WaterShaderProperty` non-BS variants) WERE plumbed in #940; the Skyrim+ siblings (`BSSkyShaderProperty`, `BSWaterShaderProperty`) were missed.

## Impact

Anyone running a Skyrim cell sees the sky dome / sun-glare / moon as magenta checker. This is the most visible NIF-import gap currently in the renderer.

## Suggested Fix

Add two `scene.get_as::<>` branches inside the `shader_property_ref` block in `material/walker.rs` alongside the BSLightingShaderProperty and BSEffectShaderProperty branches:

```rust
} else if let Some(sky) = scene.get_as::<BSSkyShaderProperty>(idx) {
    info.texture_path = intern_texture_path(pool, &sky.source_texture);
    // sky_object_type: 0=clouds, 1=sun glare, 2=moon, 3=stars; treat as opaque emissive
    info.is_sky_object = true;
    info.sky_object_type = sky.sky_object_type;
} else if let Some(water) = scene.get_as::<BSWaterShaderProperty>(idx) {
    info.uv_offset = water.uv_offset;
    info.uv_scale = water.uv_scale;
    info.has_uv_transform = true;
    info.water_shader_flags = water.water_shader_flags;
    // Note: source texture for water comes from BSShaderTextureSet via texture_set_ref (FO4+) or external water material
}
```

Source-texture → `info.texture_path`; UV pair → `info.uv_offset/uv_scale` + `has_uv_transform`; sky-shader treated as opaque emissive (no scene lighting); water_shader_flags wire later but at minimum prevent the silent texture drop. `MaterialInfo` may need new fields (`is_sky_object`, `sky_object_type`, `water_shader_flags`) — pattern after existing `material_kind` etc.

## Completeness Checks

- [ ] **SIBLING**: Are there OTHER Skyrim-era shader properties that parse cleanly but have no importer consumer? Grep `pub struct BS\w+ShaderProperty` in `crates/nif/src/blocks/shader.rs` vs `get_as::<BS\w+ShaderProperty>` in `crates/nif/src/import/`
- [ ] **RENDERER**: Sky objects (`is_sky_object = true`) need to bypass scene lighting in the fragment shader — verify the shader path exists or add it
- [ ] **TESTS**: Real-data validation — load a Skyrim cell (`--esm Skyrim.esm --cell Whiterun01`), confirm sky/sun/moon meshes have `texture_path` populated via `mesh.info`
- [ ] **PARITY**: The FO3/FNV non-BS sky/water shader consumers (#940) — confirm the new Skyrim+ branches match the same conventions, not divergent ones
- [ ] **MATERIALINFO**: Any new fields added to `MaterialInfo` must be reset to default on every fresh walk — verify the `Default` impl

## Audit reference

`docs/audits/AUDIT_NIF_2026-05-12.md` § Findings → MEDIUM → NIF-D4-NEW-03.

Related: #940 (FO3/FNV sky/water/grass shader consumers — this is the missing Skyrim+ counterpart).

