# #7374 — REN-D5-2026-11-23-01: the terrain-patch pass (TSD-4.1/4.2) has no `memory-budget.md` ledger row, and is not in "Not yet ledgered"

Snapshot of the issue as filed; GitHub is authoritative for current state.

Filed from `docs/audits/AUDIT_RENDERER_2026-11-23.md` (finding REN-D5-01). Reported by reading: nothing here was run on a GPU.

- **Severity**: LOW (doc rot; the skill's rule "a pass with none is a finding")
- **Dimension**: Memory/Lifecycle
- **Location**: `TerrainPatchPass` (`vulkan/terrain_patch.rs`: `readback`, descriptor pool/sets, the reserved patch meshes via `reserve_meshes`); `TerrainPatchBlasSet` (`acceleration/blas_terrain_patch.rs`); `docs/engine/memory-budget.md` (no match for `terrain patch`, `TerrainPatch`, `terrain_patch` or `displacement`; the only terrain row is the terrain-tile SSBO).
- **Status**: NEW (the BLAS + scratch half is REN-D1-02 in dim_1.md; this item is the pass itself and the vertex-pool share. Precedent for filing a missing ledger row as D5: #7030, #6745, both closed)
- **Description**: three resident owners arrived with TSD-4 and none is on the ledger. (1) `TerrainPatchPass::create_resources` allocates one `GpuBuffer::create_host_readback` of `READBACK_BYTES` per frame in flight = `TERRAIN_PATCH_SLOTS` (24) x `TERRAIN_PATCH_READBACK_WORDS` (2 x `TERRAIN_PATCH_EDGE_SAMPLES` 260 + 4 = 524) x 4 B = 50,304 B per slot, 100,608 B total. (2) `reserve_meshes` reserves 24 fine (4,225 vertices, 24,576 indices) + 24 coarse (1,089 vertices, 6,144 indices) patch meshes at init as the registry's first handles: 127,536 vertices x 104 B = 13.26 MB and 737,280 indices x 4 B = 2.95 MB, resident for the whole session in the global pools (device SSBO and the host `pending_vertices` mirror both carry it), admitted as Essential and never dropped. `terrain-displacement.md` §3.1 still quotes 12.9 MB for the fine lattice only (the coarse lattice came with a06d5f9db). (3) the patch BLAS set and its own scratch (REN-D1-02).
- **Evidence**: `rg -i "terrain.patch|terrain_patch|displacement" docs/engine/memory-budget.md` -> no hit; the "Not yet ledgered" list names per-mesh buffers, `StagingPool`, skin output buffers and water param SSBOs only. Constants read from `shader_constants_data.rs` (`TERRAIN_PATCH_SLOTS` 24, `TERRAIN_PATCH_VERTEX_COUNT` 4225, `TERRAIN_PATCH_COARSE_VERTEX_COUNT` 33 x 33, `TERRAIN_PATCH_QUADS` 64, `TERRAIN_PATCH_COARSE_STEP` 2, `TERRAIN_PATCH_EDGE_SAMPLES` 4 x 65).
- **Impact**: none at runtime. The vertex-pool share is inside the 7 M cap (about 1.8 % of it, ahead of the ground reserve), so the pool arithmetic is right; what is missing is the ledger statement that this slice is permanent and unevictable, plus the small readback and the BLAS figures.
- **Related**: REN-D1-02, #7221 (open), #7030.
- **Suggested Fix**: one row in the Acceleration-Structures / Mesh Registry area (pool slice from the constants above, readback 100,608 B, BLAS and scratch from `terrain.patches` stats once measured), and fix the 12.9 MB figure in `terrain-displacement.md` §3.1 to include the coarse lattice. A pin like `rough_budget_geometry_rows_match_the_pool_caps` can derive the pool slice from the constants.

**Merged from Dim 1 (REN-D1-02), the patch BLAS half of the same ledger gap:**

#### REN-D1-02: memory-budget.md has no row for the TSD-4.2 patch BLAS set (48 AS + own scratch) and the scratch never shrinks
- **Severity**: LOW (doc rot + unmeasured, non-evictable residency)
- **Dimension**: AS Correctness (memory ledger)
- **Location**: `TerrainPatchBlasSet` (`entries`, `scratch`); `AccelerationManager::total_blas_bytes`; `docs/engine/memory-budget.md` (Acceleration-Structures section; the only terrain-patch row is the terrain tile SSBO).
- **Status**: NEW
- **Description**: `create_patch_blas_entry` adds each patch AS's `buffer.size` to `total_blas_bytes` (never to `static_blas_bytes`, so the eviction compare is unaffected, same shape as skinned BLAS). The set's own scratch grows monotonically in `record_patch_blas_frame` (`scratch.size < scratch_size` -> replace, old one through `pending_destroy_scratch`) and `shrink_blas_scratch_to_fit` does not cover it. `memory-budget.md` has no row for either, nor for the reserved patch mesh pool. `terrain-displacement.md` quotes "about 13 MB" for the pool only.
- **Evidence**: `rg patch docs/engine/memory-budget.md` hits only the tile SSBO row; `blas_terrain_patch.rs` has no shrink path; `destroy_terrain_patch_blas` runs only from `AccelerationManager::destroy`.
- **Impact**: bounded (48 BLAS of 8 k triangles, scratch sized for at most 2 builds + 22 updates per frame) but not measured by me; it is VRAM the 6 GB RT-minimum budget (feedback_vram_baseline) does not see, and `blas_budget_for_heap` does not bill it (the BLAS budget is `heap/3`, so a few tens of MB is inside the noise, but the ledger should say so).
- **Related**: #7221 (open, TSD-4.2 tracking), #5365.
- **Suggested Fix**: add one row (AS bytes from `terrain.patches` stats `bytes`, scratch bytes, patch vertex pool) to the Acceleration-Structures table, measured from a live exterior run; optionally expose scratch bytes in `TerrainPatchBlasStats`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other passes, other shaders, other call sites)
