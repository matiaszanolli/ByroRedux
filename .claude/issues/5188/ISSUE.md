# #5188 — REN-D4-2026-10-03-01: Failed geometry-SSBO rebuild leaves scene-set bindings 8/9 and caustic bindings 9/10 naming destroyed buffers; ray hits keep reading them (#5064's latch is one-way, the re-point has no None arm)

**Labels**: critical,renderer,vulkan,memory,bug
**Filed from**: `docs/audits/AUDIT_RENDERER_2026-10-03.md` (audit HEAD `f002763b4`)

- **Severity**: CRITICAL. This is a GPU use-after-free (undefined behaviour) on the main shading path, not only in the caustic splat. The trigger is narrow: an allocation failure inside `rebuild_geometry_ssbo`. But the low-memory reclaim arm that makes the failure likely is the arm that frees the old pair *first*. The dimension agent filed this as HIGH and caustic-only. The orchestrator traced the scene-set half and escalated it (see Evidence).
- **Dimension**: Sync/Barriers (descriptor validity at draw/dispatch). Cross-ref Dim 5, memory lifecycle.
- **Location**:
  - `crates/renderer/src/vulkan/context/sync_and_acquire_frame.rs`: the per-frame re-point `if let (Some(vb), Some(ib)) = (global_vertex_buffer, global_index_buffer) { scene_buffers.write_geometry_buffers(..); caustic.write_geometry_buffers(..) }`. It has no else arm.
  - `crates/renderer/src/vulkan/caustic.rs`: `CausticPipeline::write_geometry_buffers` is the only writer of `geometry_bound[frame_index]`, and it stores `true` only. The reader is `CausticPipeline::geometry_bound`.
  - `crates/renderer/src/mesh/geometry_ssbo.rs`: `rebuild_geometry_ssbo_atomic_fallback` (reclaim arm: `device_wait_idle` → `global_vertex_buffer.take()` + `destroy`; non-reclaim arm: `deferred_destroy.push(.., DEFAULT_COUNTDOWN)`) → `build_geometry_ssbo`, where the `create_device_local_buffer(..)?` failure leaves both fields `None`.
  - `byroredux/src/app_frame.rs`: the `rebuild_geometry_ssbo` `Err` arm only logs `warn!`.
  - `crates/renderer/src/vulkan/context/dispatch_skin_and_cluster.rs`: `patch_camera_rt_flag(.., 0.0)` runs only on a *TLAS build* failure, never on missing global geometry.
  - `crates/renderer/src/vulkan/context/post_passes.rs`: `record_caustic_splat_pass` `match (tlas_handle, geometry_ready)`.
- **Status**: NEW. The caustic half is a residual of CLOSED #5064, whose fix covers only "never written". The scene-set half predates the window. No issue covers the rebuild-failure path; searched `rebuild_geometry_ssbo`, "geometry SSBO rebuild failure descriptor" and "caustic geometry_bound".
- **Description**:
  - **The trigger.** When `build_geometry_ssbo` fails after the old pair was taken, the session continues with `global_vertex_buffer == None`. On the reclaim arm the old pair is already destroyed. On the deferred arm it is destroyed `MAX_FRAMES_IN_FLIGHT` ticks later.
  - **No descriptor re-points or invalidates.** From that frame on, `sync_and_acquire_frame` skips both re-points. Both descriptor sets keep naming the freed `VkBuffer`s:
    - set 1 bindings 8/9 (`GlobalVertices` / `GlobalIndices`, read by `ray_hit.glsl` for every committed hit);
    - the caustic set's bindings 9/10.
  - **Scene shading keeps tracing.** The RT flag stays 1.0, because the TLAS still builds: static BLAS are self-contained and survive the geometry rebuild by design. So `triangle.frag`'s reflection, GI and glass hits reconstruct UVs and normals by indexing `vertexData[...]` through the stale binding. Bindings 8/9 are `PARTIALLY_BOUND`, but that only exempts descriptors that are *never dynamically used*. The code comment says "the None case — no geometry yet / headless — leaves them validly unbound", which is true only for the never-written case, not for a once-written descriptor whose buffer was destroyed.
  - **The caustic splat keeps dispatching.** `geometry_bound(frame)` is still `true`, and `tlas_handle(frame)` is still `Some` (#2673 keeps it).
  - **Contrast:** `record_volumetrics_pass` re-derives the live globals each frame and skips on `None`, which is the safe pattern.
- **Evidence**:
  - `grep geometry_bound crates/renderer/src` finds only `store(true` in `write_geometry_buffers`, plus the reader.
  - The re-point block in `sync_and_acquire_frame.rs` is a bare `if let` with no `else`.
  - In `dispatch_skin_and_cluster.rs`, the only `patch_camera_rt_flag(.., 0.0)` sits in the TLAS-build-failure arm.
  - `ray_hit.glsl` reads `vertexData[(vOff + i0) * VERTEX_STRIDE_FLOATS + VERTEX_UV_OFFSET_FLOATS]` on every committed hit.
  - The geometry pass already handles `None` for raster (`global_bound = false`), which shows the state is expected to be survivable.
- **Impact**:
  - Reads of freed device memory from the main fragment shader and the caustic compute, every frame until a retried rebuild succeeds.
  - Possible page fault → `VK_ERROR_DEVICE_LOST`; at best garbage RT hit attributes.
  - This violates VUID-vkCmdDispatch-None-08114 and its draw-time twin (descriptors must be valid when dynamically/statically used).
- **Related**: #5064, #2374, #3443 (reclaim arm), #2673 / #4779 (TLAS handle kept), #4843 (rt-flag lifecycle).
- **Suggested Fix**:
  - Add an else arm to the re-point. On `None`, clear `caustic.geometry_bound[frame]` and drop the frame's RT flag (`patch_camera_rt_flag(.., 0.0)`) so no shader traces against geometry it cannot reconstruct. Alternatively, keep the old pair alive until a replacement exists: build-then-swap, falling back to *not* reclaiming when the replacement allocation fails.
  - Pin both halves with `production_text` scans.
  - Confirm with a fault-injected `build_geometry_ssbo` allocation failure under `BYRO_VALIDATION=1` (Needs-RenderDoc).

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other call sites, other docs naming the same fact)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix
