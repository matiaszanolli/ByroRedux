# Issue #660: AS-8-5: current_addresses_scratch Vec freshly allocated every frame — defeats tlas_instances_scratch amortization pattern

**File**: `crates/renderer/src/vulkan/acceleration.rs:1821-1826`
**Dimension**: Acceleration Structures

`build_tlas` materializes `current_addresses_scratch: Vec<vk::DeviceAddress>` (8 B per instance) every frame for the `decide_use_update` zip-compare, then moves it into `tlas.last_blas_addresses` to cache for next frame's compare.

The capacity hint `Vec::with_capacity(instances.len())` allocates a fresh heap region per frame; the `tlas_instances_scratch` pattern (line 1522, 1998, plus `shrink_scratch_if_oversized` at 1999) goes to length to amortize the AccelerationStructureInstanceKHR scratch buffer, but the parallel u64 address Vec sits next to it un-amortized.

At 8k instances on an exterior cell that's 64 KB of heap churn per frame — 3.84 MB/s at 60 FPS — to feed a 4-byte boolean.

**Fix**: Stash a parallel `current_addresses_scratch: Vec<u64>` on `AccelerationManager` next to `tlas_instances_scratch`. Take/clear/reserve at top of `build_tlas` like the instance scratch, run the zip-compare, then assign-from-clone (or take + repopulate) into `tlas.last_blas_addresses`. Same shrink-on-oversize policy. Net win: zero per-frame heap churn for the address compare.

## Completeness Checks
- [ ] SIBLING: same pattern checked in related files
- [ ] DROP: if Vulkan objects change, verify Drop impl still correct
- [ ] LOCK_ORDER: if RwLock scope changes, verify TypeId ordering
- [ ] FFI: if cxx bridge touched, verify pointer lifetimes
- [ ] TESTS: regression test added for this specific fix

---
*From [AUDIT_RENDERER_2026-04-25.md](docs/audits/AUDIT_RENDERER_2026-04-25.md) (commit 20b8ef0)*
