# #4833 — REN-D1-2026-09-24-01: TLAS `instance_custom_index` is capped at `MAX_INSTANCES`, not at the slot's actual instance-SSBO capacity — after a failed grow, RT hit shaders index past the SSBO

**Labels**: bug,renderer,medium,vulkan
**Filed from**: docs/audits/AUDIT_RENDERER_2026-09-24.md (audited `main` @ `6c5555c70`)

- **Severity**: MEDIUM. The CRITICAL "instance_custom_index vs SSBO index" floor was considered and not applied: the mismatch cannot arise in normal operation, only after an already-logged allocation failure on a > 65,536-instance frame (same precondition class and rating as the raster-side sibling #4726). The consequence is an unchecked out-of-bounds storage-buffer read with `robustBufferAccess` off, so the merger may raise it.
- **Dimension**: AS Correctness
- **Location**: `context/begin_frame_recording.rs` — `begin_frame_recording` (`build_instance_map(.., MAX_INSTANCES, ..)`); `context/dispatch_skin_and_cluster.rs` — `dispatch_skin_and_cluster` (`build_tlas`); `context/build_and_upload_instances.rs` — `build_and_upload_instances` (the only `ensure_instance_capacity` call); `scene_buffer/upload.rs` — `upload_instances` (clamps to `instance_capacity`).
- **Status**: NEW. Sibling of open #4726 / #4722 (raster batches and UI instance); neither names the TLAS.
- **Description**: #4199 made the instance SSBO start at `INITIAL_INSTANCE_CAPACITY = 0x10000` and grow on demand. `draw_frame` order is `begin_frame_recording` (builds `instance_map` capped at the constant `MAX_INSTANCES = 0x40000`) → `dispatch_skin_and_cluster` (builds the TLAS from that map, so custom indices reach 262,143) → `build_and_upload_instances` (only here does `ensure_instance_capacity` run). If the grow fails, `upload_instances` clamps to the old capacity and drops the tail, but the TLAS already carries instances whose custom index is ≥ that capacity. Before #4199 the SSBO was `MAX_INSTANCES`-sized, so the two caps agreed.
- **Evidence**: The grow site logs and continues (`Err(e) => log::warn!("Failed to grow instance SSBOs: {e:#}")`). The `debug_assert_eq!` (#2913) compares pre-upload CPU counts and cannot see it. Shader consumers index without a bound: `raytrace.glsl` `instances[hitInstanceIdx]`, `ray_hit.glsl` (×4), `shadow_transport.glsl` (×2), `triangle.frag`, `water.frag`, `caustic_splat.comp`. Volumetrics is the one consumer that bounds-checks (`rigidBoundaryNormal`). `device.rs` has no `robust*` feature.
- **Impact**: Only when a single frame has more than 65,536 SSBO instances (the bench-of-record's largest TLAS is 13,038) and the host-visible grow allocation (first doubling ≈ 29 MB) fails; the retry repeats every frame, so the hazard persists for the scene. Every shadow / reflection / GI / caustic / water ray that hits an instance with index ≥ capacity reads undefined memory.
- **Related**: #4726, #4722, #4199, #2913.
- **Suggested Fix**: Attempt the grow before the map is built, using `draw_commands.len().min(MAX_INSTANCES)` as the requirement, and pass `min(MAX_INSTANCES, instance_capacity(frame))` as `max_kept` so a failed grow yields a TLAS that only names slots the SSBO holds. This is CPU-side allocation ordering, not a barrier or pipeline change; `build_instance_map` is pure and testable with a faked capacity.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)
- [ ] **DROP**: If Vulkan objects change, the Drop impl / teardown ordering is still correct (destroy before `allocator.take()`)
