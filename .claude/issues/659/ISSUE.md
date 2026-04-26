# Issue #659: AS-8-4: BLAS scratch alignment to minAccelerationStructureScratchOffsetAlignment never asserted

**File**: `crates/renderer/src/vulkan/acceleration.rs:506-513` (#260 R-05 comment) + every `get_buffer_device_address(scratch)` call site
**Dimension**: Acceleration Structures

Comment at line 506-513 acknowledges the BLAS scratch device address must be aligned to `VkPhysicalDeviceAccelerationStructurePropertiesKHR::minAccelerationStructureScratchOffsetAlignment` (typically 128 or 256 bytes per spec, GPU-dependent). `gpu-allocator` typically returns GpuOnly allocations at 256+ alignment because of memory-type alignment requirements, but Vulkan does not require the allocator to satisfy this AS-specific property — only the AS spec does.

Today this works on every desktop GPU because GpuOnly buffers come back >= 256B aligned. On a future driver / mobile GPU with a min-alignment of 512 or 1024, the build silently violates spec. Validation layer catches the mismatch but only if validation is on (debug builds).

**Fix**: At device init, query `min_accel_struct_scratch_offset_alignment` from `VkPhysicalDeviceAccelerationStructurePropertiesKHR` (already plumbed via `caps.ray_query_supported` from c46dc78), store on `AccelerationManager`, and either:
- (a) round up the BLAS scratch buffer's allocation size to ensure `device_address % alignment == 0` and assert it post-allocation, OR
- (b) round the device address up at use time and adjust the scratch buffer size to `build_scratch_size + alignment`.

Add a `debug_assert!(scratch_address % min_align == 0)` at every `scratch_data(...)` call site.

## Completeness Checks
- [ ] SIBLING: same pattern checked in related files
- [ ] DROP: if Vulkan objects change, verify Drop impl still correct
- [ ] LOCK_ORDER: if RwLock scope changes, verify TypeId ordering
- [ ] FFI: if cxx bridge touched, verify pointer lifetimes
- [ ] TESTS: regression test added for this specific fix

---
*From [AUDIT_RENDERER_2026-04-25.md](docs/audits/AUDIT_RENDERER_2026-04-25.md) (commit 20b8ef0)*
