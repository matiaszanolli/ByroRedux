# #1338 — D3-01: Water-plane normal-map texture refcount leaked on every cell unload

_Snapshot as filed from AUDIT_FNV_2026-05-30 (d3-01). GitHub is authoritative for live state — query `gh issue view 1338 --json state`._

**Severity**: HIGH · **Dimension**: Cell Loading · **Source**: AUDIT_FNV_2026-05-30 (D3-01)

**Location**: `byroredux/src/cell_loader/water.rs:183-201` (acquire) vs `byroredux/src/cell_loader/unload.rs:76-118` (victim walk that misses it)

**Description**: `spawn_water_plane` resolves the WATR TNAM normal map via `resolve_texture` (bumps the texture-registry refcount) but stores the index only in `WaterMaterial.normal_map_index`. The water entity carries `WaterPlane`/`WaterMaterial`/`MeshHandle` but **no `TextureHandle`/`NormalMapHandle` component**, so the unload victim walk (queries MeshHandle/TextureHandle/NormalMapHandle/DarkMapHandle/ExtraTextureMaps/TerrainTileSlot at unload.rs:76-81) can't reach it and never calls `drop_texture`.

**Evidence**: water.rs:188 `resolve_texture(...)` → `material.normal_map_index = resolved_normal_idx` (water.rs:200). unload.rs has zero references to `WaterMaterial`/`normal_map_index`. The mesh handle IS dropped; the texture is not.

**Impact**: One never-freed GPU texture + bindless slot per water-bearing cell unloaded, for the process lifetime. Self-caps at the count of distinct WATR textures but is a genuine leak in the M40 streaming / door-walk path. Same leak class the #627 terrain-slot fix + #524 refcount design were built to prevent.

**Suggested Fix**: Attach `NormalMapHandle(resolved_normal_idx)` (when `!= 0`) on the water entity; the existing unload walk (`nq` at unload.rs:78) then drops it automatically — zero new unload code.

## Completeness Checks
- [ ] **SIBLING**: Audit other entity kinds that store a texture index in a material struct rather than a handle component (e.g. terrain base texture, decal LUTs) for the same unreachable-by-unload pattern.
- [ ] **DROP**: Confirm `drop_texture` is reached exactly once for the water normal map (no double-drop if the same texture is shared with a static mesh).
- [ ] **TESTS**: Regression test — spawn a water cell, unload it, assert the normal-map refcount returned to its pre-load value.
