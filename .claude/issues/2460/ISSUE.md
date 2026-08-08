# AS-D1-NEW-01: shrink_blas_scratch_to_fit computes its peak from static BLAS only, ignoring live skinned BLAS that share the same scratch buffer

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2460
**Finding ID**: AS-D1-NEW-01 (source: `docs/audits/AUDIT_RENDERER_2026-08-07.md`)

**Severity**: HIGH
**Dimension**: 1 — AS Correctness (BLAS/TLAS)
**Location**: `crates/renderer/src/vulkan/acceleration/memory.rs::AccelerationManager::shrink_blas_scratch_to_fit` (consumers: `blas_skinned.rs::AccelerationManager::refit_skinned_blas`)
**Status**: NEW (adjacent to `#1127` / PERF-DIM7-04, closed 2026-05-24 as "stale-premise" on the memory angle; that closeout's premise is factually wrong — see Evidence — and the correctness angle was never examined)

## Description
`blas_scratch_buffer` is a single allocation shared by **both** the static BLAS builders (`build_blas`, `build_blas_batched`) **and** the skinned BLAS builder/refitter (`build_skinned_blas_batched_on_cmd`, `refit_skinned_blas`). `shrink_blas_scratch_to_fit` derives its shrink target `peak` by walking `self.blas_entries` **only**:
```rust
let peak: vk::DeviceSize = self.blas_entries.iter().flatten()
    .map(|e| e.build_scratch_size).max().unwrap_or(0);
```
`self.skinned_blas` is never consulted, even though every `BlasEntry` in it does carry a populated `build_scratch_size`. If a live skinned entity's scratch requirement exceeds the surviving static peak, the shrink reallocates the shared buffer **below** what that entity's next `refit_skinned_blas` needs. `refit_skinned_blas` performs no size validation — it takes `self.blas_scratch_buffer.as_ref()`, reads its device address, and submits `mode = UPDATE` with that address, with no `get_acceleration_structure_build_sizes` re-query. Nothing re-grows the buffer for an entity that already has a BLAS.

## Evidence
Two call sites can fire while skinned BLAS are live:
- `crates/renderer/src/vulkan/context/resize.rs::recreate_swapchain_core` — a window resize with NPCs on screen. No cell transition, so **every** skinned BLAS survives.
- `byroredux/src/cell_loader/unload.rs::finish_unload_batch` — the #1127 closeout claimed "the static-survivors peak walk is a correct lower-bound after the unload drops all skinned entries". It does not: `grep -rn drop_skinned_blas` shows the only callers are `context/skinned_blas_refit.rs` (LRU sweep + count/flag-mismatch paths) and `context/mod.rs` (shutdown). `unload_cell` merely queues `pending_skin_unload_victims`, drained on a **later** frame. At the moment `shrink_blas_scratch_to_fit` runs, the outgoing cell's skinned BLAS are still resident and counted by nothing.

Reachable shape: exterior cell grows the shared scratch to e.g. 40 MB on a large terrain/LOD BLAS → unload → interior cell whose static survivors peak at ~1 MB → `scratch_should_shrink(40 MB, 1 MB)` passes both the 2× and 16 MB-slack gate → buffer reallocated at 1 MB → a surviving NPC's `refit_skinned_blas` submits an UPDATE whose scratch range runs past the 1 MB allocation.

The `peak == 0` arm is the degenerate version: with no static survivors the scratch is dropped entirely, and `refit_skinned_blas` fails its `.context("blas_scratch_buffer absent...")?` on every skinned entity until one first-sights again — that arm fails loudly; the shrink-to-static-peak arm fails silently.

## Impact
AS build scratch overrun — the GPU writes build scratch past the end of the allocation. Consequences range from a corrupted neighbouring allocation to `VK_ERROR_DEVICE_LOST`. Blast radius is every skinned actor's RT presence (shadows/reflections/GI) plus whatever allocation follows the scratch buffer in the `gpu-allocator` slab. Invisible to `cargo test` (no live device); **needs RenderDoc / `BYRO_VALIDATION=1` verification** to confirm the driver actually faults rather than silently over-reserving.

## Related
`#1127` / PERF-DIM7-04 / REN-D2-NEW-01 (closed stale-premise, wrong premise); `AUDIT_PERFORMANCE_2026-05-19.md:88` (flagged the same peak-walk gap, framed as under-shrink/memory only); `#1782` (deferred scratch destroy — orthogonal, the *when* not the *how big*).

## Suggested Fix
Make the peak walk cover both maps: chain `self.skinned_blas.values()` into the `blas_entries` iterator when taking the `max()` of `build_scratch_size`, and apply the same union to the `peak == 0` early-drop arm. Pure CPU bookkeeping over an already-recorded field, unit-testable, no barrier/stage change. Optionally add a `debug_assert!(scratch_buffer.size >= entry.build_scratch_size)` in `refit_skinned_blas` so a future regression trips in debug instead of on the GPU.

## Completeness Checks
- [ ] **TESTS**: A unit test constructs both `blas_entries` and `skinned_blas` with differing scratch sizes and confirms the shrink peak takes the max of both
- [ ] **SIBLING**: Verify no other shared-buffer sizing path (e.g. compaction scratch) has the same map-only walk
- [ ] **DROP**: Confirm the fix doesn't change teardown/eviction ordering
