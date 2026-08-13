# REN-D5-03: two distant-LOD spawn paths leak GPU handles on their entities.is_empty() early return

- **Severity**: MEDIUM
- **Dimension**: 5 — Memory/Lifecycle
- **Location**: `byroredux/src/cell_loader/object_lod.rs` (`spawn_object_lod_quad`) and `byroredux/src/cell_loader/placement_lod.rs` (`spawn_placement_lod_cell`). Correct sibling: `byroredux/src/cell_loader/terrain_lod_btr.rs` (the upload-failure arm).

## Description
Both functions acquire refcounted GPU resources into locals, then bail through an `entities.is_empty()` guard that discards the locals without releasing them. `World::despawn` has no GPU side effects and the returned block is `None`, so nothing downstream can perform the matching release — `unload_object_lod_block` / `unload_placement_lod_block` never see these handles. `spawn_object_lod_quad` resolves the worldspace object atlas **once, before** the per-sub-mesh loop, and every sub-mesh can then be skipped (empty positions/indices) or fail its upload. `spawn_placement_lod_cell` is worse: `mesh_handles` and `texture_handles` are declared **before** the group loop and accumulate uploaded global-SSBO mesh ranges and texture refcounts across *all* groups, so a `.lod` whose groups all carry `count == 0` (the parser accepts it) strands every one of them.

## Evidence
`object_lod.rs` — `let atlas = resolve_texture(ctx, tex_provider, Some(atlas_path.as_str()));` sits above the `for mesh in &imported.meshes` loop, and the only `drop_texture` for it is in `unload_object_lod_block`, reachable only via a returned `ObjectLodBlock`. `placement_lod.rs` — the three `Vec::new()` declarations precede the group loop; the pushes run inside it; the `entities.is_empty()` return follows with no release. `terrain_lod_btr.rs` proves the contract is understood at this layer: its upload-failure arm calls `drop_texture` on both handles with the comment *"Release the refs the two resolves above acquired, or a failed upload pins their VkImages + bindless slots for the session (the #1537 leak shape)."*

Spot-checked against live code during publish: `object_lod.rs:252` (`resolve_texture`) and `:317-323` (`if entities.is_empty() { return None; } Some(ObjectLodBlock { ... })` with no drop) confirmed. `placement_lod.rs:430-431` (handle Vecs declared before the loop) and `:563-570` (same early-return shape) confirmed. `terrain_lod_btr.rs:260-266` confirmed the correct release pattern exists in the sibling.

## Impact
Stranded bindless texture slots and stranded global vertex/index SSBO ranges that no reclaim path can ever free. Slot-space exhaustion is the documented slow-motion failure mode for `TextureRegistry` (#2030 — grow-only slot space makes each stranded slot permanent); the mesh side pins pool bytes against `VERTEX_POOL_SOFT_CAP` / `INDEX_POOL_SOFT_CAP`. The object-LOD case is a *stuck refcount* rather than runaway growth (every quad in a worldspace resolves the same atlas path). Reachability is genuinely narrow — a `.bto` whose every sub-mesh is degenerate or whose uploads all fail under memory pressure; a `.lod` with only zero-count groups — hence MEDIUM rather than the HIGH floor.

## Related
#1537 (the original LOD texture-refcount leak), #2030 / MEM-D3-01, #2374 / EX-08 (the exterior resource-ownership soak — the natural place to assert this).

## Suggested Fix
Release before each early return, mirroring `terrain_lod_btr.rs`: in `spawn_object_lod_quad` drop `atlas` (when non-zero and not the fallback) on the `entities.is_empty()` path; in `spawn_placement_lod_cell` drain `mesh_handles` through `drop_mesh` and `texture_handles` through `drop_texture` on the same path. Cheapest durable guard: extend the #2374 ownership soak to assert `live_static_blas_count` / `live_slot_count` return to baseline after a LOD block that spawns nothing.

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2758
