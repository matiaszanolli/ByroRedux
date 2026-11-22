# #7263: Terrain-patch pass still allocates per-frame Vecs outside the BLAS path (#7246 follow-up)

**Severity**: LOW · **Found by**: /audit-sync 2026-11-22 (performance group; follow-up to #7246)

## Description
#7246 made the patch **BLAS** record path reuse its per-frame lists (`PatchBlasFrame`). The rest of the terrain-patch frame path still allocates fresh `Vec`s every exterior frame while patches are resident:
- `TerrainPatchPass::record` (`crates/renderer/src/vulkan/terrain_patch.rs`): `pushes` and `dispatched`, each `Vec::with_capacity(self.frame_slots.len())`.
- `TerrainPatchPass::harvest`: a fresh `words: Vec<u32>` collected from the readback bytes.
- Host side (`byroredux/src/terrain_patches.rs`): `assign_patches` (`candidates`, `resident`) and `push_terrain_patches` (`slots`).

## Impact
A handful of small heap allocations per frame; not measured as a cost, but the same class #7246 removed, and unguarded (no dhat / allocation-reuse test).

## Suggested Fix
Keep these as working-set fields cleared per frame (the `PatchBlasFrame` / `emptied` pattern from #7246), and extend `the_record_paths_frame_lists_keep_their_allocations`-style guards to them.

## Completeness Checks
- [ ] **SIBLING**: host and device sides both covered
- [ ] **TESTS**: a guard pins that the lists keep their allocations across frames
