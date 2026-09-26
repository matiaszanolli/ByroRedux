# REN-D5-2026-09-26-04: The core `buffer.rs` constructors do not unwind on allocator failure; the geometry rebuild retries them every frame

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/4882

**Labels**: medium,renderer,vulkan,memory,bug

- **Severity**: MEDIUM (one-shot per failure, amplified to per-frame by the retry loop; escalate to HIGH if fault injection confirms the bind and submit arms)
- **Dimension**: Memory/Lifecycle
- **Location**: `crates/renderer/src/vulkan/buffer.rs` — `StagingPool::acquire` (`allocate(..)?` leaks the VkBuffer; `bind_buffer_memory(..)?` leaks buffer and allocation), `GpuBuffer::create_host_visible`, `create_host_readback`, `create_device_local_uninit`, `create_device_local_buffer` (allocate, bind, and `with_one_time_commands(..)?` arms). The unwinding siblings are `create_staging_buffer` (#2164) and `create_empty_device_local_buffer` (#3298).
- **Status**: NEW (sibling gap of closed #2164; #4854/finding 02 is the same class in `texture.rs`)
- **Description / Evidence**:
  - Each constructor calls `create_buffer`, then `allocator.allocate(..)?`, then `bind_buffer_memory(..)?` with a bare `vk::Buffer` local (a `Copy` handle with no `Drop`). I confirmed this in `create_host_visible`: there is no destroy on the allocate or bind error arms. `GpuBuffer::Drop` cannot help because `Self` is never built.
  - `staging_guard_coverage_tests` exempts `buffer.rs` as the legitimate creator, so nothing pins the unwind.
  - Amplification: `SceneBuffers::ensure_instance_capacity` documents "on allocation failure the slot keeps its current buffers", and `grow_instance_ssbos` runs twice per frame. `byroredux/src/app_frame.rs` calls `mesh_registry.rebuild_geometry_ssbo` every frame while a rebuild is in progress or dirty, and only `warn!`s on `Err`. A persistent failure therefore leaks at least one VkBuffer per attempt per frame, and the bind or submit arms leak real allocations, including the full global SSBO in `build_geometry_ssbo`.
- **Impact**: Handle leak per failed allocate. Real device or host memory per failed bind or submit.
- **Related**: #2164, #3298, #4854.
- **Suggested Fix**: Factor one `create_bound_buffer` helper that owns the unwind (destroy on allocate error; destroy and free on bind error) and route the five sites through it. Give `create_device_local_buffer` a guard that also covers the copy-submit failure arm. Pin it with a source-scan like `staging_guard_coverage_tests`. The bind and submit arms need a `BYRO_VALIDATION=1` fault-injection run.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **DROP**: If Vulkan objects change, the Drop impl and teardown ordering are still correct
- [ ] **SIBLING**: Same pattern checked in related files (sibling constructors / upload paths / docs)
- [ ] **TESTS**: A regression test pins this specific fix
