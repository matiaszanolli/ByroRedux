# #3991 — REN-2026-09-06-D4-01: the skin/BLAS chain commits record-time state that `draw_frame`'s three tail `Err` sites never roll back — `skin_dispatch_ran` is a record-time latch used as a submit-time signal

**Labels**: medium, renderer, shaders, sync, bug

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D4-01), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: MEDIUM
- **Dimension**: Sync/Barriers
- **Location**: `crates/renderer/src/vulkan/context/skinned_blas_refit.rs`
  (`VulkanContext::record_skinned_blas_refit` — the `self.skin_dispatch_ran = true`
  statement and its "#1796 / D6-02" justification comment),
  `crates/renderer/src/vulkan/acceleration/blas_skinned.rs`
  (`AccelerationManager::build_skinned_blas_batched_on_cmd`, Phase 4),
  `crates/renderer/src/vulkan/scene_buffer/upload.rs`
  (`SceneBuffers::record_bone_world_copy`'s trailing
  `mark_bone_world_slot_written` loop),
  `byroredux/src/app_frame.rs` (the
  `if !ctx.skin_dispatch_ran || ctx.bind_inverse_upload_failed` rollback gate)
- **Status**: NEW
- **Description**: `#917` established this codebase's rule for the frame tail:
  temporal state advances **after** `queue_submit` returns success, never at
  recording time. `draw_frame` applies it to SVGF, TAA, volumetrics, FSR and the
  rigid-model history swap — all of which sit *below* the `queue_submit` call.
  The skin / skinned-BLAS chain never got the same treatment, and its guard flag
  is the wrong shape to give it one.

  `record_skinned_blas_refit` sets `self.skin_dispatch_ran = true`
  unconditionally at its top, with the rationale *"reaching this function at all
  proves `draw_frame` got past both early-return guards"*. That was true of the
  hazards it was written for (#1796) and of the ones #2522 widened it to (fence
  wait, command-buffer begin, FSR parameter build — all *above* this point). It
  is not true of `draw_frame`'s three **tail** `Err` sites, which sit *below* it:
  `end_command_buffer`, `reset_fences`, and `queue_submit`. `draw.rs`'s own
  `#3837` comment describes exactly those three as *"the same recovery paths
  #910 hardened, so reachable in practice on swapchain churn"*. On any of them
  the command buffer is discarded and nothing recorded this frame executes — yet
  `skin_dispatch_ran == true`, so `app_frame.rs`'s rollback does not fire.

  Four pieces of state have already been committed by then, all describing GPU
  work that will never run:

  1. **`SkinSlotPool` pose-hash commits** stay committed, so next frame's
     `pose_dirty` is empty for those entities and no re-upload is scheduled.
  2. **`bone_world_slot_states[slot]`** — `record_bone_world_copy` calls
     `mark_bone_world_slot_written(state, frame_index)` in a loop *after*
     recording the copy. Once the sibling FIF slot's bit lands on a later
     successful frame, `mark_bone_world_slot_written` resets the byte to `0`
     (fully clean) and `bone_world_device_buffers[frame_index]` is left holding
     the pre-update pose until something re-dirties the slot.
  3. **`SkinSlot::has_populated_output = true`**, set immediately after the
     `skin_vertices.comp` dispatch is *recorded*, which is what the next frame's
     `#1196` skip gate reads.
  4. **`AccelerationManager::skinned_blas`** — `build_skinned_blas_batched_on_cmd`
     Phase 4 inserts the `BlasEntry` (with `built_flags: SKINNED_BLAS_FLAGS`)
     after recording the BUILD. `has_skinned_blas(entity)` therefore returns
     `true` for an acceleration structure whose backing memory was never written.
     `refit_skinned_blas` then takes the `mode(UPDATE)` /
     `src_acceleration_structure(entry.accel) == dst_acceleration_structure(entry.accel)`
     path against it on every subsequent frame, and `build_tlas` publishes its
     device address into the TLAS for ray queries to traverse. Item 4 is the one
     with spec weight — see *Needs-RenderDoc N-3*.
- **Evidence**:
  - `record_skinned_blas_refit` — `self.skin_dispatch_ran = true;` precedes the
    `if let (Some(skin_pipeline), Some(ref mut accel))` gate; the comment above
    it enumerates only the two early-return guards.
  - `draw_frame` — after `self.dispatch_skin_and_cluster(...)` (which calls
    `record_skinned_blas_refit`) the function still contains three
    `return Err(e)` sites: the `end_command_buffer` arm inside the tail `unsafe`
    block, the `reset_fences` arm, and the `queue_submit` arm. Each calls
    `recreate_image_available_for_frame` (and the last also
    `recreate_in_flight_for_frame`) and returns — the sync objects are healed,
    the skin state is not.
  - `byroredux/src/app_frame.rs` — the rollback is
    `if !ctx.skin_dispatch_ran || ctx.bind_inverse_upload_failed { … }`;
    its `#2522` comment enumerates only Err sites that execute *before*
    `record_skinned_blas_refit`.
  - `app_frame.rs`'s `Err(e) =>` arm logs `"Draw failed"` and calls
    `event_loop.exit()` — **queued**, not immediate. `draw.rs`'s own `#1211`
    guard comment establishes that a `RedrawRequested` already in flight still
    reaches `draw_frame` after such an exit is queued, which is what makes the
    post-failure frames reachable at all.
  - The contrasting correct pattern is 30 lines below in `draw_frame`:
    `svgf.mark_frame_completed()` / `taa.mark_frame_completed()` /
    `volumetrics.mark_frame_completed()` / `mark_dispatch_completed()` /
    the `previous_rigid_models` swap, all gated on `queue_submit` having
    returned `Ok` (#917).
- **Impact**: Bounded to the frames between a tail `Err` and process teardown,
  but within that window: a skinned actor renders from a stale bone palette in
  one of the two FIF buffers (alternating-frame pose pop), its GPU-skinned
  vertex buffer is treated as populated when it is not, and its BLAS is
  UPDATE-refit and ray-traced from memory that was never built. Severity
  arbitrated to MEDIUM rather than the `/audit-severity` HIGH floor for a Vulkan
  spec violation because the whole window sits inside an already-fatal error
  path with exit queued; if N-3's validation-layer check confirms the
  UPDATE-against-never-built path fires, HIGH is the right reading.
- **Related**: #917 (the pattern this chain never adopted), #1796 / #2522 /
  #3569 (three successive widenings of the same rollback gate, none of which
  reached the tail sites), #910 / #952 (which hardened the sync objects on
  exactly these three sites), #1211 (queued-exit reachability), #3837.
- **Needs RenderDoc**: only for the item-4 half — see N-3. Items 1–3 are plain
  CPU state machines, fully decidable from source.
- **Suggested Fix**: **No barrier or pipeline change.** Split the record-time
  latch from the submit-time signal: keep `skin_dispatch_ran` as the
  "recording reached the skin section" flag it is, and either (a) widen
  `app_frame.rs`'s rollback to fire on `draw_result.is_err()` as well, or
  (b) mirror #917 — move the four commits (`mark_bone_world_slot_written`,
  `has_populated_output`, the `skinned_blas` insert, and the pool's pose-hash
  commit) behind a post-`queue_submit` promotion, as
  `VolumetricsPipeline`'s own `pending_simulation_time_seconds` →
  `last_simulation_time_seconds` promotion already does. Either is
  `cargo test`-pinnable in the existing style of
  `skin_dispatch_ran_rollback_scope_tests`.

---

## Completeness Checks

- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **SPIRV**: If GLSL changed, recompile the `.spv` (`cd crates/renderer/shaders && glslangValidator -V -I. <shader> -o <shader>.spv`) and commit it
- [ ] **TESTS**: A regression test pins this specific fix
