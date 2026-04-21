# Investigation — Issue #495 (PERF D2-M1)

## Domain
Renderer — `crates/renderer/src/vulkan/acceleration.rs` + `byroredux/src/cell_loader.rs` (invocation site).

## Premise verification (per audit hygiene)

✓ **Confirmed**. `scratch_needs_growth` at line 207 is strictly monotonic:

```rust
fn scratch_needs_growth(current_capacity: Option<vk::DeviceSize>, required: vk::DeviceSize) -> bool {
    match current_capacity {
        Some(cap) => cap < required,  // never shrinks
        None => true,
    }
}
```

Four call sites all grow-only: single BLAS build (`:416-431`), batched BLAS build (`:660-675`), per-frame TLAS build (`:1221-1247`).

`evict_unused_blas` (`:1467-1524`) drops BLAS entries + bumps `total_blas_bytes` but never touches `blas_scratch_buffer`. Once a single peek at a large mesh pins the scratch buffer at 80–200 MB DEVICE_LOCAL, it stays pinned for process lifetime.

## Constraints

- **BLAS scratch**: one shared buffer, used during BLAS BUILDS only. Cell-unload fences on no in-flight BLAS build, so drop+recreate at `unload_cell` is safe.
- **TLAS per-frame scratch**: `scratch_buffers[i]` is used every frame for the TLAS rebuild. Dropping it at `unload_cell` without device_wait_idle is unsafe (frame `i` may still be reading it on the GPU). Would need a pending-destroy queue mirroring `pending_destroy_blas`. **Out of scope** for this fix — documented as follow-up.
- `BlasEntry` doesn't currently store `build_scratch_size`; can't compute the surviving peak without re-querying `get_acceleration_structure_build_sizes`. Adding the field at build time is the cheap fix.

## Plan

1. Add `build_scratch_size: vk::DeviceSize` to `BlasEntry`.
2. Populate at the two BLAS build sites (single `:509` and batched `:953`).
3. Add pure helper `scratch_should_shrink(current, peak)` — threshold: `current > 2 × peak AND current − peak > 16 MB slack`.
4. Add `unsafe fn shrink_blas_scratch_to_fit(&mut self, device, allocator)`:
   - Compute peak = `max(build_scratch_size)` across surviving `BlasEntry`s.
   - If `scratch_should_shrink(current, peak)`: destroy current, reallocate to peak (or drop to `None` if no survivors).
5. Call from `byroredux/src/cell_loader.rs:228` after the `drop_blas` loop.
6. Unit tests for `scratch_should_shrink` (5 cases: empty/below-threshold/2x-exact/above-threshold/zero-peak).

## Scope
2 files. `acceleration.rs` gets the structural changes; `cell_loader.rs` gets the one-line invocation.

## TLAS scratch deferred
Tracked in code comment + commit message as a follow-up requiring the pending-destroy pattern to be safe. Not in this fix.
