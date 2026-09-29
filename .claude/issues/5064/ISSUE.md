# #5064 — CONC-D2-2026-09-29-02: Caustic set bindings 9/10 are never written when there is no global geometry SSBO, but the dispatch still runs (`VUID-vkCmdDispatch-None-08114`)

**Labels**: high,sync,vulkan,renderer,bug

**Source report**: `docs/audits/AUDIT_CONCURRENCY_2026-09-29.md`
**Severity**: HIGH
**Dimension**: Compute → AS → Fragment Chains

## Location
- `crates/renderer/src/vulkan/context/sync_and_acquire_frame.rs` — `caustic.write_geometry_buffers` inside `if let (Some(vb), Some(ib)) = (global_vertex_buffer, global_index_buffer)`
- `crates/renderer/src/vulkan/caustic.rs` — layout bindings 9/10, no `PARTIALLY_BOUND`
- `crates/renderer/src/vulkan/context/post_passes.rs` — `record_caustic_splat_pass`, gated on TLAS only

## Description
The CI error names a two-set (per-FIF) compute descriptor set whose bindings 9 and 10 were never updated. Four compute layouts have set-0 bindings 9/10: SVGF temporal and volumetrics inject write both unconditionally at creation; ground-cover models writes all bindings right before a dispatch that is gated off without ground cover; **caustics** writes 9/10 only when both global geometry buffers exist. In the demo scene they do not: `spawn_demo_primitives` uploads via `MeshRegistry::upload`, which never calls `accumulate_global_geometry`, so `build_geometry_ssbo` early-returns on empty `pending_vertices`. The scene set's bindings 8/9 handle this `None` case by being `PARTIALLY_BOUND` ("validly unbound"); the caustic layout has no such flag and its dispatch is gated on a TLAS only.

## Evidence
CI job 109545444434, 10 errors on `VkDescriptorSet 0x1cb…` / `0x1cc…`:
```
[ VUID-vkCmdDispatch-None-08114 ] … vkCmdDispatch(): the descriptor (VkDescriptorSet 0x1cb00000001cb[], binding 9,
index 0) is being used in draw but has never been updated via vkUpdateDescriptorSets() or a similar call.
```
(and binding 10, on both sets). Pipeline attribution is by elimination — confirm with a local `BYRO_VALIDATION=1` run of the bare demo or debug-utils object names on the sets. Not reproduced on the 4070 Ti with a real cell (RT validation capture: 0 errors).

## Impact
Undefined descriptor contents in a live compute dispatch. `caustic_splat.comp` dereferences `GlobalVertices`/`GlobalIndices` for committed-hit reconstruction, so any dynamic access is UB (device fault on strict drivers). Trigger: RT on, TLAS built, no mesh through `upload_scene_mesh*` (bare demo, any scene before its first global-geometry build). Certain cost: the only validation gate stays red.

## Related
CONC-D2-2026-09-29-01 (#5062), #4987.

## Suggested Fix
Skip the caustic dispatch until the geometry bindings have been written for that slot (e.g. a per-FIF latch set in `write_geometry_buffers`), or bind a small placeholder buffer at creation, or mark 9/10 `PARTIALLY_BOUND` as the scene set does. Confirm on the next lane run.

Validated at HEAD 9fcfdc3fc: `caustic.write_geometry_buffers` still inside the `(Some(vb), Some(ib))` guard; no `PARTIALLY_BOUND` in `caustic.rs`; caustic dispatch keyed on `tlas_handle(frame)`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix
