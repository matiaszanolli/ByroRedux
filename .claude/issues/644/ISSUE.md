# Issue #644: MEM-2-2: BLAS scratch buffer used outside one-time-command fence in skinned BLAS path

**File**: `crates/renderer/src/vulkan/acceleration.rs:611-720` (build_skinned_blas) + `acceleration.rs:803-885` (refit_skinned_blas) + `acceleration.rs:394-588` (build_blas)
**Dimension**: GPU Memory / Sync

`blas_scratch_buffer` is shared across three callers — `build_blas` (one-time cmd + fence), `build_skinned_blas` (one-time cmd + fence), and `refit_skinned_blas` (records into the per-frame `cmd`). The first-sight skinned path is sync via `with_one_time_commands_reuse_fence`, which serializes against itself.

But if a skinned entity's first-sight BUILD lands in the **same** frame as the per-frame `refit_skinned_blas` for any other entity, both consumers reference `blas_scratch_buffer` — the BUILD's submit completes before refit's command buffer executes (fence wait inside the one-time helper), but there is no Vulkan barrier between the BUILD's AS_BUILD_WRITE on the scratch region and refit's AS_BUILD_READ. The fence covers host-side visibility but not GPU pipeline ordering against the per-frame cmd.

**Fix**: Either give skinned builds their own scratch buffer, or insert an `ACCELERATION_STRUCTURE_BUILD_KHR → ACCELERATION_STRUCTURE_BUILD_KHR` `MemoryBarrier` on the scratch range at the top of every per-frame skinned-refit dispatch when a sync BUILD ran the same frame.

## Completeness Checks
- [ ] SIBLING: same pattern checked in related files
- [ ] DROP: if Vulkan objects change, verify Drop impl still correct
- [ ] LOCK_ORDER: if RwLock scope changes, verify TypeId ordering
- [ ] FFI: if cxx bridge touched, verify pointer lifetimes
- [ ] TESTS: regression test added for this specific fix

---
*From [AUDIT_RENDERER_2026-04-25.md](docs/audits/AUDIT_RENDERER_2026-04-25.md) (commit 20b8ef0)*
