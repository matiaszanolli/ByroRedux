# FO4-D7-01: FO4/Skyrim baked .bto object LOD materializes through the texture-only NIFAL boundary while holding a real ImportedMaterial

Issue: https://github.com/matiaszanolli/ByroRedux/issues/4245

**Severity**: MEDIUM
**Dimension**: 7 — NIFAL Canonical Material Translation (FO4)
**Location**: `byroredux/src/cell_loader/object_lod.rs:327,396-404,448-456`
**Status**: NEW

**Description**: `object_lod.rs` (the FO4/Skyrim baked `.bto` distant object LOD path) runs a full `import_nif_scene_with_resolver`, so each sub-mesh arrives with a populated `ImportedMaterial` — and since #3412 the spawner *reads* `mesh.material.textures.base_color` to pick the sub-mesh's authored diffuse — but then discards the rest of it, inserting `translate_texture_only_material(tex_path)` instead of `translate_material(&mesh.material, …)`. `translate_texture_only_material`'s own doc defines its population as draws with "no source material record" — false here, since there is one and the neighbouring lines already consume part of it. Everything else the sub-mesh authored is dropped: `alpha_test`/`alpha_threshold`, `two_sided`, `material_kind`, `effect_shader_flags` (no `PBR_BSDF`/`MODEL_SPACE_NORMALS` routing), emissive, vertex-color mode, `z_write`, UV scale/offset, normal map — and no `AlphaBlend`/`TwoSided`/`IsDecalMesh` marker is attached either.

**Evidence**: Confirmed in current code — `object_lod.rs:399-404` reads `mesh.material.textures.base_color` to resolve `authored`, but `object_lod.rs:449-458` still calls `crate::material_translate::translate_texture_only_material(...)` rather than `translate_material(&mesh.material, ...)`. Contrast `placement_lod.rs:542` (the Oblivion-only sibling scheme), which *does* call `translate_material` on its imported LOD sub-mesh material — the two LOD schemes disagree, and the one that says "no material to translate" is the FO4/Skyrim one.

**Impact**: Every FO4/Skyrim baked object-LOD draw shades from keyword classification of the diffuse path alone. The consequential loss is alpha coverage: an alpha-tested cutout baked into a `.bto` (fences, railings, billboard foliage) renders as an opaque quad at LOD distance — a silhouette change, not a subtle shading pop. Secondary: lost `two_sided` flips backface culling for single-plane LOD geometry, and the missing `PBR_BSDF` bit routes FO4 LOD through the legacy lighting lobe while the full-res mesh beside it takes the Disney one. Blast radius is the whole exterior LOD ring on both titles — distant geometry, hence MEDIUM rather than HIGH; escalates to HIGH if a census of real `.bto` alpha-test/two-sided usage comes back nonzero (verification recipe needs targeted archive extraction, not run yet).

**Related**: #2444/MAT-D3-02, #3412, `translate_texture_only_material`'s doc block and its #4043 correction.

**Suggested Fix**: Call `translate_material(&mesh.material, …)` here, the same shape `placement_lod.rs` already uses, keeping the atlas-fallback texture resolution as-is, and add `attach_blend_and_facing_markers`.

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: The fix keeps per-game logic at the NIFAL parser→`Material` boundary (`translate_material`), never re-derived at render time — see `/audit-nifal`.
- [ ] **SIBLING**: Confirm `placement_lod.rs`'s `translate_material` call pattern is followed exactly, including `attach_blend_and_facing_markers`
- [ ] **TESTS**: A regression test on a `.bto` fixture with an alpha-tested/two-sided sub-mesh asserts the resulting `Material`/markers match a full mesh spawn of the same source material
