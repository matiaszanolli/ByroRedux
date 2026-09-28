# #4978: REN-D12-2026-09-27-02: `render.debug probe` resolves the hit entity through a later frame's SSBO→entity map, so it can name the wrong entity

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4978
- **Labels**: low,renderer,tech-debt,bug

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-27.md` (full renderer audit, all 12 dimensions, audited `main` @ `7e9da5dcc`; code at the finding's location unchanged at `fe80f4d76`). Finding ID: **REN-D12-2026-09-27-02**._

- **Severity**: LOW
- **Dimension**: Debug/Telemetry
- **Location**:
  - `crates/renderer/src/vulkan/context/render_debug.rs:45` (`selected_ray_hit_entity_id`);
  - caller `byroredux/src/app_frame.rs:1255-1258` (`apply_pending_debug_requests`);
  - map maintenance `crates/renderer/src/vulkan/acceleration/tlas.rs:500` and `:744` (`tlas_entity_ids_scratch`);
  - field doc `crates/renderer/src/vulkan/render_debug.rs:146` (`committed_hit_entity_id`).
- **Status**: NEW (`de808add3`)
- **Description**:
  - The probe is traced in frame N. Its record is read back in `sync_and_acquire_frame` of frame N+`MAX_FRAMES_IN_FLIGHT`, after that slot's fence.
  - The result is taken in the *next* `apply_pending_debug_requests`, which runs before that frame's `draw_frame`.
  - At that point `committed_hit_instance` (an instance-SSBO index) goes through `tlas_entity_ids_scratch`. That is the single manager-level map filled by the most recent TLAS gather, about two frames after the trace.
  - SSBO indices are the compacted per-frame draw order. `tlas.rs`'s own comment says "SSBO indices can change freely with raster order".
- **Evidence**: `selected_ray_hit_entity_id` does `accel_manager.tlas_entity_ids_scratch.get(instance_ssbo_index)`. Nothing keys the lookup to the frame the probe was armed in. `tlas_entity_ids_scratch.resize(draw_commands.len(), 0)` does not clear older entries either.
- **Impact**:
  - Any camera motion, streaming or cull change between the trace and the resolve can print a valid but unrelated entity id.
  - The field doc says the id is "absent if the matching TLAS membership has already changed". It is not absent; it is wrong.
  - This is the output used for the single-sided-wall and shadow-leak hunts, so an occluder can be misattributed.
  - With a static camera the order is usually stable, so the bug is intermittent.
- **Related**: `ab255cfd2`/`186234944` (the ReSTIR light-identity work, same diagnostic family).
- **Suggested Fix**: When a probe is armed (a one-shot, so the cost is bounded), snapshot `tlas_entity_ids_scratch` (or just the TLAS-gather generation) into the armed-request slot for that frame. Resolve against that snapshot at readback, or return `None` when the generation differs. Fix the field doc to match.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)
