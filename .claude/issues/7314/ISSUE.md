# #7314 ECS-D1-01: `terrain_patches::nearby_cells` takes `TerrainTileSlot` before `MeshHandle`, inverting the render collector and cell unload

Labels: high,ecs,concurrency,terrain-exterior,bug

From `docs/audits/AUDIT_ECS_2026-11-22.md` (HEAD e2a4c54cc; re-verified at f5329b2f5).

- **Severity**: HIGH. It is an ECS lock-order inversion on a per-frame production path. The guards are read locks on the main thread, so it cannot deadlock today.
- **Dimension**: 1 Lock Ordering
- **Location**: `byroredux/src/terrain_patches.rs` `nearby_cells`, called from `emit_patch_draws` inside `build_render_data` (`byroredux/src/render/mod.rs`).
- **Status**: NEW. It came in with the TSD-4.1 patch host (#7152) and was reworked by #7263. No open or closed issue matches.
- **Description**: `nearby_cells` takes `(TerrainCellOrigin, TerrainTileSlot, MeshHandle)` in one tuple and holds all three. That records the edge `TerrainTileSlot → MeshHandle`. Two other paths take the opposite order:
  - The static-mesh collector holds `MeshHandle` while taking `TerrainTileSlot` (`render/static_meshes.rs` `StaticMeshQueries::new`, `let mq = world.query::<MeshHandle>()` and later `let terrain_tile_q = world.query::<TerrainTileSlot>()`).
  - `cell_loader/unload.rs` `collect_victim_gpu_handles` (`mq` then `ttq`) does the same.

  Together these close a 2-cycle in the process-wide graph. Patches are on by default, so every exterior frame records both directions once the pool meshes arrive.
- **Evidence**: `let (Some(origins), Some(tiles), Some(meshes)) = (world.query::<TerrainCellOrigin>(), world.query::<TerrainTileSlot>(), world.query::<MeshHandle>())` against `static_meshes.rs:393/454` and `unload.rs:843/848`.
- **Why the lane is green**: no test world that runs `build_render_data` has a `TerrainTileSlot` storage. `World::query` returns `None` before it arms the tracker, so no edge is recorded.
- **Impact**: any live exterior run under `BYRO_LOCK_ORDER_CHECK=1` panics in the detector. That also hides the next real cycle. Release builds are not affected.
- **Related**: #6209 (the same `collect_victim_gpu_handles` family); `dfcbcafd5`.
- **Suggested Fix**: take `MeshHandle` before `TerrainTileSlot` in `nearby_cells`, so the order is `TerrainCellOrigin, MeshHandle, TerrainTileSlot`. That is also compatible with ground cover's `TerrainCellOrigin → … → MeshHandle`. Add a fixture with all three storages that runs `nearby_cells` and the static collector under the detector.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other multi-storage gathers / other rows / other producers)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, the process-wide order (and TypeId-sorted pair acquisition) is preserved; the `BYRO_LOCK_ORDER_CHECK=1` lanes (core, scripting, bin) stay green
- [ ] **TESTS**: A regression test pins this specific fix (for a lock-order fix: a fixture that holds every storage involved, so the detector lane can see the edge)
