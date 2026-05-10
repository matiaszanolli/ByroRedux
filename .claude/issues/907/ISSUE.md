---
issue: 0
title: REN-D12-NEW-01: refit_skinned_blas accepts re-derived vertex/index counts; entity remap → primitiveCount VUID
labels: bug, renderer, M29, high, vulkan
---

**Severity**: HIGH (Vulkan VUID, undefined BVH state)
**Source audit**: docs/audits/AUDIT_RENDERER_2026-05-09.md (Dim 12)

## Location

- `crates/renderer/src/vulkan/acceleration.rs:1016-1095` — `refit_skinned_blas` accepts `vertex_count: u32, index_count: u32` parameters
- `crates/renderer/src/vulkan/acceleration.rs:823, 858` — initial BUILD takes `vertex_count` from `mesh_registry`
- `crates/renderer/src/vulkan/context/draw.rs:646-666` — refit call site re-derives counts from `mesh_registry` each frame using `entity_id` → `mesh_handle`

## Why it's a bug

`refit_skinned_blas` re-derives `vertex_count` / `index_count` from `mesh_registry` each frame using `entity_id` → `mesh_handle`. If that mapping ever changes (mod swap, future LOD switch, mesh hot-reload), `mode = UPDATE` ships a `primitiveCount` that mismatches the original BUILD.

Per Vulkan spec — VUID-vkCmdBuildAccelerationStructuresKHR-pInfos-03667:
> "If the `mode` member of any element of `pInfos` is `VK_BUILD_ACCELERATION_STRUCTURE_MODE_UPDATE_KHR`, the corresponding `pBuildRangeInfos[i][j].primitiveCount` for each j must equal the corresponding `pBuildRangeInfos[i][j].primitiveCount` used to build the source acceleration structure"

Driver behavior is undefined; on NVIDIA the BVH silently corrupts.

## Fix sketch

1. Add `built_vertex_count: u32` and `built_index_count: u32` fields to `BlasEntry` (and skinned-BLAS entry struct).
2. Set them at BUILD time inside `build_skinned_blas` / `build_blas`.
3. In `refit_skinned_blas`, `debug_assert_eq!(self.built_vertex_count, vertex_count)` and same for index_count.
4. If they ever diverge in release, force a fresh BUILD instead of UPDATE (graceful degrade, not silent corruption).

## Repro

Currently no in-engine path triggers the remap (M41-EQUIP outfit swap reuses the same skel mesh). The bug would manifest when:
- A mod swaps a skinned NPC's mesh at runtime
- A future LOD system hot-swaps to a lower-poly variant
- Mesh hot-reload during dev iteration

VK_LAYER_KHRONOS_validation will catch it the moment it happens.

## Completeness Checks

- [ ] **UNSAFE**: refit_skinned_blas already unsafe; debug_assert is safe-side check.
- [ ] **SIBLING**: Static `build_blas` UPDATE path (acceleration.rs:583-734) — same invariant; pin `built_vertex_count` there too.
- [ ] **DROP**: No Drop impact.
- [ ] **LOCK_ORDER**: No RwLock changes.
- [ ] **FFI**: No cxx changes.
- [ ] **TESTS**: Add a unit test that BUILDs with vertex_count=N then attempts UPDATE with vertex_count=M and asserts the assertion fires (debug) or graceful BUILD fallback (release).

## Related

- #661 SY-4 (skin compute → BLAS refit barrier wrong access mask) — separate bug, same code path.

🤖 Filed by /audit-publish from docs/audits/AUDIT_RENDERER_2026-05-09.md
