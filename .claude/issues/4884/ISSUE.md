# REN-D5-2026-09-26-06: BLAS scratch is retired before its replacement exists — a failed reallocation leaves `blas_scratch_buffer == None` while live skinned BLAS need it

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/4884

**Labels**: medium,renderer,vulkan,memory,bug

- **Severity**: MEDIUM (error path; degraded RT on animated actors until another build path allocates scratch)
- **Dimension**: Memory/Lifecycle
- **Location**: `crates/renderer/src/vulkan/acceleration/memory.rs` — `shrink_blas_scratch_to_fit` (`pending_destroy_scratch.push(old)`, then `create_device_local_uninit`, whose `Err` arm says "leave `blas_scratch_buffer` as `None` … degraded but correct"); `crates/renderer/src/vulkan/acceleration/blas_static.rs` — `build_blas_batched` Phase 2 (same order, then `?`); `crates/renderer/src/vulkan/acceleration/blas_skinned.rs` — `build_skinned_blas_batched_on_cmd` (immediate old destroy, then allocate). Consumer: `refit_skinned_blas` (`blas_scratch_buffer absent`).
- **Status**: NEW
- **Description / Evidence**:
  - I confirmed the ordering in both `shrink_blas_scratch_to_fit` and Phase 2. The old buffer is moved into `pending_destroy_scratch` first, then the allocation is attempted.
  - "Degraded but correct" holds only when no BLAS survives. The shrink deliberately sizes to the union peak over static and skinned BLAS, so it is reachable with skinned BLAS live. After a failed realloc, `record_skinned_blas_refit` gets `Err` per dirty entity per frame, and `refit_count` (bumped only on success) never reaches the forced-rebuild limit.
  - The TLAS sibling was fixed with allocate-then-swap (#2915). Deferral (#1782) fixes lifetime, not the `None` state.
- **Impact**: Raster is unaffected. RT shadows, reflections and GI of animated NPCs freeze at their last BLAS pose, with per-frame WARN spam, in the VRAM-pressure regime. Recovery needs some other path to allocate scratch.
- **Suggested Fix**: Allocate the replacement first and retire the old buffer only on success, as `shrink_tlas_scratch_to_fit` does. In `build_skinned_blas_batched_on_cmd` keep the old scratch on `Err`. The immediate free stays valid there because of `draw_frame`'s all-slots wait.

## Completeness Checks
- [ ] **DROP**: If Vulkan objects change, the Drop impl and teardown ordering are still correct
- [ ] **SIBLING**: Same pattern checked in related files (sibling constructors / upload paths / docs)
- [ ] **TESTS**: A regression test pins this specific fix
