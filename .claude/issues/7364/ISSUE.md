# #7364 — CONC-2026-11-23-05

Snapshot of the issue as filed; GitHub is authoritative for current state.

Filed from `docs/audits/AUDIT_CONCURRENCY_2026-11-23.md` (finding CON-05).

- **Severity**: LOW (test-coverage gap; the code is correct today)
- **Dimension**: Resource Lifecycle
- **Location**: `crates/renderer/src/vulkan/context/teardown.rs:146-149` (`patches.destroy(&self.device, alloc)`), `crates/renderer/src/vulkan/acceleration/mod.rs:538` (`self.destroy_terrain_patch_blas(device, allocator)`)
- **Status**: NEW. Closest closed issue: #6521 / SAFE-2026-10-27-01 (a `SceneBuffers` family missing from `destroy`). Searched `terrain patch teardown`, `destroy coverage`.
- **Description**: both calls exist and sit before `allocator.take()` and `Arc::try_unwrap`; none of the three load-bearing orderings is involved. No test names either. `SceneBuffers::destroy` is protected by `destroy_walks_every_per_frame_buffer_family`, but `TerrainPatchPass` and `TerrainPatchBlasSet` own handles with no `Drop` (`vk::Pipeline`, `vk::DescriptorPool`, `vk::AccelerationStructureKHR`). Their `GpuBuffer` fields self-free through `Drop` but hold an allocator `Arc` clone, so a dropped call leaks the AS and pipeline handles and trips the outstanding-references path at `teardown.rs:495-508`, the failure shape of SAFE-2026-10-27-01.
- **Evidence**: `rg "destroy_terrain_patch_blas|patches.destroy"` over `crates/renderer/src` returns the two production call sites only; `acceleration/tests/*.rs` has no destroy-coverage test.
- **Trigger Conditions**: a refactor that moves or deletes either call. Shutdown only.
- **Impact**: a shutdown leak plus a debug-build assertion; no per-frame effect.
- **Related**: #6521, `skin_slot_drain_sits_outside_the_skin_compute_guard` (the pattern to copy).
- **Verification Path**: cargo test (source-shape scan over `production_text(include_str!("teardown.rs"))` and `acceleration/mod.rs`).
- **Suggested Fix**: one source-scan test naming both call sites.
- **Dimension agent ID**: CON-D6-01

## Completeness Checks
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix
