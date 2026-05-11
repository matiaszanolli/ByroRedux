# NIF-D4-NEW-01: Import walker drops file_name from TileShader/SkyShader/WaterShader/TallGrass*

**Severity**: MEDIUM
**Source audit**: `docs/audits/AUDIT_NIF_2026-05-10.md` (Dim 4)

## Game Affected

FO3, FNV (HUD tiles, sky, grass, water), Oblivion (some sky), Skyrim (`BSSkyShader` / `BSWaterShader`)

## Location

`crates/nif/src/import/material/walker.rs:438-811` — the per-prop dispatch loop in `extract_material_info_from_refs`

## Why it's a bug

Parser-side fixes shipped in #455 (TileShader), #474 (Water), #550 (Sky) added dedicated structs with `file_name` fields. The import walker was never updated to consume them.

`grep "TileShader\|SkyShader\|WaterShader\|TallGrass"` in the walker returns **zero hits**. The walker only dispatches `BSLightingShaderProperty`, `BSEffectShaderProperty`, `NiTexturingProperty`, `BSShaderPPLightingProperty`, `BSShaderNoLightingProperty`, `NiStencilProperty`, `NiFlagProperty`, `NiVertexColorProperty`.

Parser-side `TileShaderProperty` doc (`shader.rs:206-218`) explicitly notes: "Pre-#455 ... HUD overlays (stealth meter, airtimer, quest markers) lost their texture path as a result." The parser fix shipped, the importer never followed.

## Impact

FO3/FNV HUD tiles, sky domes, water surfaces, tall grass meshes silently fall through with `info.texture_path = None`. They render as the magenta-checker placeholder — same "Chrome posterized walls" failure mode noted in the user feedback memory, but on sky/UI surfaces. Sky and water visibly affected; HUD compositor still uses a separate path so it's less obvious there.

## Fix

Add `scene.get_as::<TileShaderProperty>(idx)` / `SkyShaderProperty` / `WaterShaderProperty` / `TallGrassShaderProperty` / `BSSkyShaderProperty` / `BSWaterShaderProperty` arms inside the per-prop loop. For each, route `file_name` into `info.texture_path` when not already set. For `SkyShaderProperty`, surface `sky_object_type` on `MaterialInfo` for the sky-pipeline dispatch when it lands.

Pattern matches the existing `BSShaderNoLightingProperty` arm at `walker.rs:695`.

## Completeness Checks

- [ ] **SIBLING**: All 6 shader-property types added (Tile, Sky, Water, TallGrass, BSSky, BSWater)
- [ ] **TESTS**: Extend `mesh_material_path_capture_tests.rs` with one fixture per shader type
- [ ] **REGRESSION**: Verify with `tex.missing` console command on FNV exterior cell or FO3 sky mesh after fix lands
