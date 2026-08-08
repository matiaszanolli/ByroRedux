# REN-D9-NEW-02: pending_skin_unload_victims drain and the SkinSlot LRU sweep are gated behind the global vertex SSBO being present

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2494
**Finding ID**: REN-D9-NEW-02 (source: `docs/audits/AUDIT_RENDERER_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 9 — Skinning
**Location**: `crates/renderer/src/vulkan/context/skinned_blas_refit.rs:84` (guard) enclosing `:610-649` (cleanup)
**Status**: NEW

## Description
The cell-unload victim drain (#1003) and the idle-slot LRU sweep (#643 / MEM-2-1) both live inside `if let (Some((input_buffer, input_size)), Some(bone_buf)) = (global_vert_buf, bone_buffer)`. `global_vert_buf` is `self.mesh_registry.global_vertex_buffer`, which `MeshRegistry` legitimately leaves as `None` — it is `take()`n during a geometry-SSBO rebuild and is `None` before the first upload. On any frame where the global vertex buffer is absent, no `SkinSlot` is destroyed and no skinned BLAS is dropped, even for entities the cell loader has already despawned.

## Evidence
Cleanup block `skinned_blas_refit.rs:608-649` is nested three levels inside the `Some(global_vert_buf)` guard opened at `:84`; the only other consumer of `pending_skin_unload_victims` is `byroredux/src/cell_loader/unload.rs:207` (producer).

## Impact
Bounded, not unbounded — the next frame with a live global vertex buffer drains the backlog. Worst case is that GPU memory for freed actors' output buffers + BLAS, and their descriptor-pool slots, stay held across a cell transition window. Matters most on the exact frames where memory headroom is tightest (the low-headroom `device_wait_idle` rebuild path).

## Related
#1003 (drain), #643 / MEM-2-1 (LRU sweep), #900 (`failed_skin_slots` un-suppression, which also only fires from inside this block).

## Suggested Fix
Hoist the eviction/drain block out of the `(global_vert_buf, bone_buffer)` guard — it needs only `skin_pipeline`, `accel` and `alloc`, all of which are already in scope from the outer `if let`.

## Completeness Checks
- [ ] **TESTS**: A regression test confirms the drain runs on a frame where `global_vertex_buffer` is `None`
