# NIF-DIM4-NEW-02: WaterShaderProperty (FO3/FNV legacy) parsed but never consumed at import

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/1243

**Source**: `docs/audits/AUDIT_NIF_2026-05-23.md` (Dim 4)
**Severity**: LOW (orphan-parse)
**Dimension**: Import Pipeline
**Game Affected**: FO3 / FNV

## Description

The FO3/FNV-era `WaterShaderProperty` (non-BS variant) is parsed via its dedicated arm at `crates/nif/src/blocks/mod.rs:460` (added in #474 to fix the over-read against `BSShaderPPLightingProperty::parse`), but `crates/nif/src/import/material/walker.rs` has no `get_as::<WaterShaderProperty>(idx)` site. The legacy walker consumes `TileShaderProperty`, `SkyShaderProperty`, and `TallGrassShaderProperty` (#940 wire-up) but `WaterShaderProperty` was missed in the same pass. Its `env_map_scale` (the only meaningful field on the body) never reaches `MaterialInfo`.

## Evidence

```
$ grep -rn "get_as::<WaterShaderProperty>" crates/nif/src/import/
(no matches)
```

The Skyrim+ sibling `BSWaterShaderProperty` is consumed at `walker.rs:480-488` (#977 closure). The FO3/FNV non-BS variant has no parallel.

## Impact

On FO3/FNV the cell-side WATR / water-shader path is the dominant water render route; the per-mesh `WaterShaderProperty` is rare in vanilla content but appears on legacy water plane meshes. The mesh-driven water surfaces on those NIFs render with `env_map_scale = 0.0` (no env reflection contribution), the same `MaterialInfo::default()` failure mode fixed by #773 / FO3-4-02 for the `BSShaderPPLightingProperty` path. Corpus is small enough that severity stays LOW.

## Suggested Fix

4-line consumer in `walker.rs` after the `TallGrassShaderProperty` arm:

```rust
if let Some(shader) = scene.get_as::<WaterShaderProperty>(idx) {
    info.env_map_scale = shader.shader.env_map_scale;
}
```

Requires adding `WaterShaderProperty` to the `crate::blocks::shader::*` import at the top of `walker.rs`.

## Related

- #940 (CLOSED): Tile/Sky/TallGrass wire-up companion (same code path)
- #474 (CLOSED): parser correctness for this block
- #773 (CLOSED, FO3-4-02): the same `env_map_scale` gap fixed on PPLighting / NoLighting paths
- #977 (CLOSED): BSWaterShaderProperty wire-up (Skyrim+ sibling — used as template)

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: confirm `BSWaterShaderProperty` consumer at `walker.rs:480-488` still handles its surface the same way after this change (no shared state surprise)
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: synthetic FNV-era NIF fixture with a `WaterShaderProperty` block; assert `MaterialInfo::env_map_scale` non-zero after import