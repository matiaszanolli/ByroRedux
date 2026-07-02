# CONC-D1-01: Shared BLAS build-scratch buffer destroyed host-side while an in-flight frame's skinned-BLAS refits still reference its device address

_Filed as #1782 from `docs/audits/AUDIT_CONCURRENCY_2026-07-02.md`._

**Severity**: CRITICAL · **Dimension**: Vulkan Queue & AS Sync · Source: `AUDIT_CONCURRENCY_2026-07-02` (CONC-D1-01)

## Location
- `crates/renderer/src/vulkan/acceleration/blas_static.rs:683-698` (`build_blas_batched` grow path: `old.destroy(device, allocator)` at :690)
- `crates/renderer/src/vulkan/acceleration/blas_static.rs:280-296` (`build_blas` single-shot grow path, same shape)
- `crates/renderer/src/vulkan/acceleration/memory.rs:42-104` (`shrink_blas_scratch_to_fit`: immediate `old.destroy` at :65 and :81; **stale SAFETY doc** at :33-39)
- Exposed call sites: `byroredux/src/cell_loader/unload.rs` (unload-time shrink, stale SAFETY comment), `byroredux/src/cell_loader/exterior.rs`, `byroredux/src/cell_loader/spawn.rs` (streaming-time `ctx.build_blas_batched`), reached from `App::step_streaming` in `about_to_wait` — before `draw_frame` in the same tick, with no `device_wait_idle` between them.

## Description
`AccelerationManager::blas_scratch_buffer` is shared by three writers: cell-load `build_blas`/`build_blas_batched` (one-time, host fence-waited), per-frame `build_skinned_blas_batched_on_cmd` (first-sight builds, #911), and per-frame `refit_skinned_blas` (recorded into the frame command buffer every pose-dirty frame; scratch device address captured at record time, `blas_skinned.rs:520-530`). `draw_frame` waits both in-flight fences at the TOP of the frame, so during recording no frame is in flight — but after `queue_submit` returns, frame N executes asynchronously on the GPU. Cell streaming (`step_streaming`, worldspace-transition drain) runs in `about_to_wait` **between** `draw_frame` calls, while frame N may still be executing. In that window:
1. `build_blas_batched` Phase 2 growth takes and **immediately destroys** the old scratch buffer (`vkDestroyBuffer` + `gpu-allocator` free, no deferral).
2. `unload_cell` → `shrink_blas_scratch_to_fit` does the same on the shrink/drop paths.

If frame N recorded any skinned-BLAS refit or first-sight build (any animating NPC on screen — the steady-state case in populated cells), its `cmd_build_acceleration_structures` calls reference the destroyed buffer's device address as build scratch. The host-side free is ordered by nothing: a dedicated allocation → `VkDeviceMemory` freed → GPU page fault → `VK_ERROR_DEVICE_LOST`; a sub-allocation → range returns to pool and a later streaming-tick allocation can be recycled onto it and race frame N's in-flight scratch access → silent memory corruption.

## Evidence
```rust
// blas_static.rs (build_blas_batched Phase 2, runs from step_streaming in about_to_wait)
if need_new_scratch {
    if let Some(mut old) = self.blas_scratch_buffer.take() {
        old.destroy(device, allocator);          // <-- immediate vkDestroyBuffer + free
    }
    self.blas_scratch_buffer = Some(GpuBuffer::create_device_local_uninit( ... )?);
}
```
```rust
// memory.rs:33-39 — the SAFETY premise this relies on, now stale (verified live):
///   The two build paths use one-time command buffers with synchronous fence
///   waits, so any call site that is NOT inside a BLAS build is safe by construction.
```
The premise is false since M29/#911: `refit_skinned_blas` and `build_skinned_blas_batched_on_cmd` capture the scratch device address into the **per-frame** command buffer, which is in flight whenever streaming runs.

## Impact
GPU use-after-free — device loss (`VK_ERROR_DEVICE_LOST`, matching the #1449 signature) or silent BLAS/scene corruption. Impact class per the severity scale: use-after-free = CRITICAL.

Trigger (all must coincide): (1) ≥1 skinned entity refit/built in the most recent submitted frame; (2) `step_streaming`/worldspace-transition unload runs before the next `draw_frame` fence wait; (3) grow arm: incoming cell mesh `build_scratch_size` exceeds session high-water mark, OR shrink arm: `unload_cell` drops the peak-scratch mesh and `scratch_should_shrink` fires (or `peak == 0` drop-entirely arm).

## Related
Sibling of closed #1449 (`a476b256`) — that fix rerouted `BlasEntry` destruction through `pending_destroy_blas` in this exact window but did NOT cover the shared scratch buffer. Cross-confirmed by Dimension 6 (CONC-D6-02). The identical grow-destroy in `build_skinned_blas_batched_on_cmd` (`blas_skinned.rs:212-214`) is **SAFE** as-is — it runs during `draw_frame` recording, after the both-slot fence wait.

## Suggested Fix
Route retired scratch buffers through the existing deferred-destroy machinery instead of `old.destroy(...)`: add a `DeferredDestroyQueue<GpuBuffer>` (`deferred_destroy.rs` already generic, `DEFAULT_COUNTDOWN = MAX_FRAMES_IN_FLIGHT`) on `AccelerationManager`, push the old buffer in all three sites (`build_blas`, `build_blas_batched`, `shrink_blas_scratch_to_fit`), drain in `tick_deferred_destroy` (post-fence-wait in `draw_frame`) and in `drain_pending_destroys` for shutdown (#732 parity). CPU-side lifetime management (unit-testable, not a barrier change) so the speculative-Vulkan-fix guardrail does not block it. Update the stale SAFETY docs in `memory.rs` + `byroredux/src/cell_loader/unload.rs` in the same change; add a distinguishing comment on the safe `build_skinned_blas_batched_on_cmd` grow-destroy so the fix isn't blindly copied there.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: All three destroy sites (`build_blas`, `build_blas_batched`, `shrink_blas_scratch_to_fit`) fixed; safe `build_skinned_blas_batched_on_cmd` grow-destroy annotated, not changed
- [ ] **DROP**: Deferred-destroy drain runs at shutdown (`drain_pending_destroys`) so no scratch buffer leaks past teardown
- [ ] **TESTS**: A regression test pins the deferred-destroy countdown for retired scratch buffers
