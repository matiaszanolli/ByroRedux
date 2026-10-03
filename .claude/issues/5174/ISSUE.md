# #5174: EXT-D3-2026-10-02-02: The #4907 ground-colour coupling depends on the terrain tile; cells with a BTXT base but no ATXT have no tile, so a new blade root-colour step appears at their borders

**Labels**: low,terrain-exterior,shaders,bug
**Source**: docs/audits/AUDIT_EXTERIOR_2026-10-02.md

- **Severity**: LOW (visual)
- **Dimension**: Ground-cover pipeline
- **Location**:
  - `byroredux/src/cell_loader/terrain.rs:1116` (a tile is allocated only when `!splat_layers.layers.is_empty()`) and `:1243-1245` (`terrain_tile_slot` sentinel).
  - `crates/renderer/shaders/groundcover_blade.vert:411-413`.
  - `crates/renderer/shaders/groundcover_blade.frag:151-160`.
- **Status**: NEW (gap in #4907's fix)
- **Tier Violated**: n/a
- **Game Affected**: all LAND games
- **Description**:
  - The original audit suggested carrying the BTXT diffuse index into the ground-cover cell record. #4907 put it in the `GpuTerrainTile` slot instead (`base_diffuse_index`).
  - A cell whose four quadrants share one base and that has no ATXT gets no tile. Its blades skip the coupling block entirely (`vTerrainTileSlot == GROUNDCOVER_NO_TERRAIN_TILE`).
  - Before #4907, blades on zero-paint ground got no coupling in either kind of cell (`weightSum == 0`). Now they couple to the base colour in painted cells but not in no-ATXT cells.
  - The result is a straight root-colour step along every border between the two kinds of cell, even when both show the same base texture.
  - #4903's affinity is not affected: `base_affinity` rides `GpuGroundCoverCell`, not the tile.
- **Evidence** (Python LAND census):
  - **Skyrim.esm**: 866 of 15,564 LAND records have an authored BTXT and no ATXT. Another 5,309 have no BTXT at all (default land).
  - **FalloutNV.esm**: 164 records have a BTXT and no ATXT, plus 23,840 default-land records (mostly outside the playable area).
  - No cell without ATXT has differing quadrant bases (0 in both games), so every such cell gets no tile.
- **Impact**: A root-tint seam along cell borders in exterior regions near no-ATXT cells, at about 5.6% of Skyrim's LAND.
- **Suggested Fix**:
  - Carry `base_diffuse_index` on `GpuGroundCoverCell` (`pad2` is free), or allocate a tile for every LAND cell.
  - Then let the blade run `byroTerrainSplatAlbedo` with zero weights wherever a base exists.

**Source report**: `docs/audits/AUDIT_EXTERIOR_2026-10-02.md` (HEAD `c9f95283a`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other games sharing the decoder / other consumers of the same field / other copies of the stale text)
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the EXAL/WATAL `translate_*` (or `spatial_units::normalize`) boundary — never pushed into shaders/renderer, never re-derived at render time
- [ ] **TESTS**: A regression test pins this specific fix
