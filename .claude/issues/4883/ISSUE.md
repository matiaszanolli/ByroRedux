# REN-D5-2026-09-26-05: `build_blas_batched` Phases 1–3 early `?` exits abandon `prepared` (raw AS handles and result buffers)

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/4883

**Labels**: medium,renderer,vulkan,memory,bug

- **Severity**: MEDIUM (error path under BLAS/VRAM pressure)
- **Dimension**: Memory/Lifecycle
- **Location**: `crates/renderer/src/vulkan/acceleration/blas_static.rs` — `AccelerationManager::build_blas_batched`: Phase 1 `GpuBuffer::create_device_local_uninit(..)?` (result buffer for mesh k), Phase 2 `create_device_local_uninit(scratch)?`, Phase 3 `create_query_pool(..).context("Failed to create compaction query pool")?`
- **Status**: NEW (sibling gap of closed #1097 / #316 / #2926)
- **Description / Evidence**:
  - I confirmed the shape by reading. The AS-create failure arm right below Phase 1 unwinds `prepared` (`for mut p in prepared { destroy_acceleration_structure; p.buffer.destroy }`, the #1097 fix), but the `create_device_local_uninit(..)?` immediately above it does not.
  - On a Phase-1 failure at mesh k > 0, `prepared[0..k]` is dropped. Each `GpuBuffer` hits the #656 Drop net (WARN plus `debug_assert!(false)`, so a panic in debug builds), and each `vk::AccelerationStructureKHR` (no `Drop`) leaks, with the AS left alive over a freed buffer.
  - Phase 2 and Phase 3 do the same for the whole batch.
- **Impact**: AS handle leak (and a debug-build panic) exactly when the admission gate or budget is being stressed. The caller treats `Err` as "RT loses the tail", so the leak repeats on every retried batch.
- **Suggested Fix**: Wrap the three exits in the same unwind the later phases use (a small `unwind_prepared(prepared, query_pool)` helper).

## Completeness Checks
- [ ] **DROP**: If Vulkan objects change, the Drop impl and teardown ordering are still correct
- [ ] **SIBLING**: Same pattern checked in related files (sibling constructors / upload paths / docs)
- [ ] **TESTS**: A regression test pins this specific fix
