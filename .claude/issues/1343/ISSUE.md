# #1343 — D3-02: Terrain splat-layer texture refcounts leaked on early returns

_Snapshot as filed from AUDIT_FNV_2026-05-30 (d3-02). GitHub is authoritative for live state — query `gh issue view 1343 --json state`._

**Severity**: MEDIUM · **Dimension**: Cell Loading · **Source**: AUDIT_FNV_2026-05-30 (D3-02)

**Location**: `byroredux/src/cell_loader/terrain.rs:273` (acquire) vs `terrain.rs:350` + `:369` (early returns before the tile-slot binds them)

**Description**: `spawn_terrain_mesh` calls `build_cell_splat_layers` (terrain.rs:273), resolving up to 8 LTEX→TXST splat textures (refcount bumps), *before* two early-return guards: `let allocator = ctx.allocator.as_ref()?` (terrain.rs:350) and the mesh-upload `Err => return None` (terrain.rs:369). On either early return, no `TerrainTileSlot` is allocated (terrain.rs:409 `allocate_terrain_tile`), so the splat-layer refcounts have no owner the unload walk can reach.

**Evidence**: Acquisition (273) precedes both early returns (350, 369), which precede the tile-slot bind (409). The unload slot-free path only reaches layer indices via `free_terrain_tile(slot)`. No compensating release exists between 273 and the spawn.

**Impact**: Up to 8 pinned texture refcounts per **failed** terrain-cell load (bindless slots + VkImages). Trigger paths are degraded states (allocator absent in headless/test; `upload_scene_mesh` returning Err on OOM / mesh-budget cap), so not steady-state — but a real leak when it fires.

**Suggested Fix**: Move the `build_cell_splat_layers` call below the mesh-upload + allocator guards (it has no dependency on the mesh), matching the water-plane ordering convention; OR push the resolved splat `texture_index`es to `drop_texture` on each early-return path.

## Completeness Checks
- [ ] **SIBLING**: Check the base-texture resolve (terrain.rs:378) and any other pre-guard acquisition for the same leak-on-early-return shape.
- [ ] **TESTS**: Regression test — force a terrain mesh-upload failure (or no-allocator) and assert splat refcounts are released.
