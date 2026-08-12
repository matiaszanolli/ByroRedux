# CONC-D1-NEW-01: TLAS resize destroys the old AS before allocating its replacement

**Issue**: #2673
**Filed**: 2026-08-12 via `/audit-publish` from `/audit-suite renderer-deep`


- **Severity**: CRITICAL
- **Dimension**: 1 — Vulkan Queue & AS Sync
- **Location**: [tlas.rs](crates/renderer/src/vulkan/acceleration/tlas.rs)`:695-790`
  (`AccelerationManager::ensure_tlas_state`); consumers
  [draw.rs](crates/renderer/src/vulkan/context/draw.rs)`:2190-2224` (`draw_frame`),
  [draw.rs](crates/renderer/src/vulkan/context/draw.rs)`:1628-1633` (`rt_flag`),
  [descriptors.rs](crates/renderer/src/vulkan/scene_buffer/descriptors.rs)`:169-185`
  (`SceneBuffers::write_tlas`)
- **Status**: NEW (no match in the dedup baseline; nearest neighbours #2481 CLOSED — *BLAS* slot
  overwrite — and #2297 OPEN — TLAS eligibility filter — are different defects)
- **Description**: `ensure_tlas_state` takes the destroy-then-allocate order. It `take()`s the
  slot, calls `destroy_acceleration_structure(old.accel)` plus three `GpuBuffer::destroy`s, and
  only afterwards runs the two fallible allocations (`GpuBuffer::create_host_visible(...)?`,
  `GpuBuffer::create_device_local_uninit(...)?`) and the AS creation. Any `?` in that window
  leaves `self.tlas[frame_index] == None` and propagates `Err` out of `build_tlas`.

  `draw_frame`'s call site treats that as non-fatal —
  `if let Err(e) = accel.build_tlas(..) { log::warn!("TLAS build failed: {e}"); }` — and the
  `else` arm that carries the AS-write→read barrier, `scene_buffers.write_tlas`, and
  `patch_camera_rt_flag` is skipped. Scene descriptor set binding 2 therefore keeps naming the
  `VkAccelerationStructureKHR` that was just destroyed.

  The failure is not self-limiting for the frame that hits it, because `rt_flag` is derived from
  `SceneBuffers::tlas_written[frame]`, and `tlas_written` is a **one-way latch**: `write_tlas`
  sets it `true` (`descriptors.rs:175`) and nothing ever clears it. Once a slot has ever had a
  TLAS, `rt_flag = 1.0` on every later frame, so `triangle.frag` (and `water.frag`) initialise
  ray queries against the dangling binding-2 handle. The render pass still executes; only
  `caustic` / `volumetrics` self-gate on `tlas_handle(frame) == None`.
- **Evidence**:
  ```rust
  // tlas.rs:719-732 — destroy first …
  if let Some(mut old) = self.tlas[frame_index].take() {
      let _ = device.device_wait_idle();
      self.accel_loader.destroy_acceleration_structure(old.accel, None);
      old.buffer.destroy(device, allocator);
      old.instance_buffer.destroy(device, allocator);
      old.instance_buffer_device.destroy(device, allocator);
  }
  // tlas.rs:768-784 — … allocate second, with two `?` exits
  let mut instance_buffer = GpuBuffer::create_host_visible(device, allocator, padded_size, ..)?;
  let mut instance_buffer_device = GpuBuffer::create_device_local_uninit(device, allocator, padded_size, ..)?;
  ```
  ```rust
  // draw.rs:2190-2199 — warn-only, barrier + write_tlas live in the else arm
  if let Err(e) = accel.build_tlas(&self.device, alloc, cmd, draw_commands, &instance_map, frame) {
      log::warn!("TLAS build failed: {e}");
  } else { /* memory_barrier(...); write_tlas(...); patch_camera_rt_flag(...) */ }
  ```
  ```rust
  // descriptors.rs:175 — one-way latch, never reset
  self.tlas_written[frame_index] = true;
  ```
  Independently re-verified for this merge: `grep -rn "tlas_written" crates/renderer/src/`
  returns exactly one assignment on the `SceneBuffers` latch (`descriptors.rs:175`, `= true`)
  and **no** reset. The `volumetrics.rs` latch of the same name is a separate field that *does*
  reset (`volumetrics.rs:1492`) — see guard #26.
- **Impact**: Use-after-free of a `VkAccelerationStructureKHR` read by every RT-shading path
  (shadows, reflections, GI, water refraction) for as long as the allocation keeps failing on
  that frame-in-flight slot. Realistic outcome is a GPU page fault → TDR →
  `VK_ERROR_DEVICE_LOST`; the benign outcome is garbage BVH traversal. The trigger condition
  (device-local allocation failure at TLAS grow time) is precisely the VRAM-pressure regime the
  BLAS budget + LRU eviction machinery exists to survive, so **this is the failure mode that
  fires exactly when the engine is trying to degrade gracefully.**
- **Trigger Conditions**: A frame whose `instance_count` exceeds the slot's `max_instances`
  (`need_new_tlas == true` — i.e. a cell load crossing the 8192 `MIN_TLAS_INSTANCE_RESERVE`, or
  the very first grow) **and** a device-local / host-visible allocation failure for
  `padded_size` (VRAM exhaustion, BAR exhaustion, `OUT_OF_HOST_MEMORY`). Reproducible
  deterministically by fault-injecting a failure return from either `GpuBuffer::create_*` inside
  `ensure_tlas_state`, in the same style as the existing `BYRO_FSR_FORCE_DISPATCH_FAIL` hook.
- **Verification Path**: Not observable in `cargo test` (no headless device). **Validation
  layer**: a `BYRO_VALIDATION=1` release run with the fault injected reports the
  destroyed-object-in-descriptor message at draw time
  (`VUID-vkCmdDrawIndexedIndirect-None-08114` family — "Descriptor in binding #2 index 0 is
  using acceleration structure … that is invalid or has been destroyed"). Without fault
  injection, natural repro needs a VRAM-starved exterior stream; **RenderDoc** would show
  binding 2 as an unresolvable AS handle.
- **Related**: **REN-D5-02 (HIGH) from the concurrent `/audit-renderer` run — the same
  underlying defect, reached independently from a different dimension. Two independent agents
  converging on one defect from different directions is the strongest evidence this suite
  produced.** Also #2481 (CLOSED, BLAS-side sibling of the same "replace without
  destroying/ordering" class); CONC-D1-NEW-02 below shares the "commit before the operation
  succeeds" root cause.
- **Suggested Fix**: Allocate the replacement buffers + AS into locals first and only destroy
  the old `TlasState` once every fallible step has succeeded (allocate-then-swap). As defence in
  depth, make `tlas_written[frame]` two-way: clear it (and re-upload `rt_flag = 0.0` via the
  existing `patch_camera_rt_flag`) whenever `build_tlas` returns `Err`, so the frame degrades to
  non-RT shading instead of reading a dead handle.

---

### HIGH


---
*Filed from [`docs/audits/AUDIT_CONCURRENCY_2026-08-12.md`](docs/audits/AUDIT_CONCURRENCY_2026-08-12.md) — `/audit-suite renderer-deep`, 2026-08-12. Finding ID `CONC-D1-NEW-01`.*

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other pipelines, other AS paths)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix
