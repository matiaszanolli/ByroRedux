# #4036 — REN-2026-09-06-D4-06: `signal_temporal_discontinuity`'s `previous_rigid_models.clear()` is inert at all three of its in-`draw_frame` call sites

**Labels**: low, renderer, sync, bug

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D4-06), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: Sync/Barriers
- **Location**: `crates/renderer/src/vulkan/context/mod.rs`
  (`VulkanContext::signal_temporal_discontinuity`, the trailing
  `self.previous_rigid_models.clear();` and its comment), against its three
  in-frame callers: `context/post_passes.rs` (`record_taa_pass`'s Err arm, #3605,
  and `record_upscale_pass`'s, #2519) and
  `context/assemble_camera_and_lights.rs` (the `camera_cut` arm)
- **Status**: NEW
- **Description**: The clear carries an explicit contract — *"The first frame
  after a discontinuity must not encode object motion against transforms from
  the retired scene/camera history."* That contract is delivered for the fifteen
  out-of-frame callers (`streaming_helpers.rs`, `debug_load.rs`, `save_io.rs`,
  `app_step.rs`, `resize.rs`), which run between frames. None of the three
  in-`draw_frame` callers gets it:
  - The two `post_passes.rs` sites run during the post-pass tail, *before*
    `draw_frame`'s unconditional
    `std::mem::swap(&mut self.previous_rigid_models, &mut current_rigid_models);`
    — which immediately refills the map with this frame's transforms. The clear
    is overwritten within the same function.
  - The `assemble_camera_and_lights.rs` site runs early enough to take effect,
    but is redundant: `build_and_upload_instances`'s `previous_source` selection
    is already gated `if uses_rigid_history && !camera_cut`, so on a cut every
    instance falls back to `m` regardless of the map's contents.

  No live defect is claimed. For both `post_passes.rs` sites the transforms are
  *not* stale (the hazard #3605/#2519 address is jitter, and motion vectors are
  reconstructed from the un-jittered projection), and the four other effects of
  `signal_temporal_discontinuity` — `svgf_recovery_frames`,
  `taa.signal_history_reset()`, `fsr.signal_reset()`,
  `volumetrics.signal_history_reset()` — all persist correctly and are what
  actually protect the recovery frame.
- **Evidence**:
  - `crates/renderer/src/vulkan/context/mod.rs::signal_temporal_discontinuity`
    ends with `self.previous_rigid_models.clear();`.
  - `draw_frame` performs the swap unconditionally on the success path, after
    `record_post_passes` and after `queue_submit`; `record_taa_pass` and
    `record_upscale_pass` are both reached from `record_post_passes`.
  - `build_and_upload_instances` — `let previous_source = if uses_rigid_history
    && !camera_cut { … } else { m };`.
- **Impact**: A five-line API where one line silently does nothing at three of
  its eighteen call sites — precisely the three that run inside `draw_frame`. The risk is a future in-frame caller added on the
  belief the clear is effective — e.g. one added below the swap, or one where
  the transforms genuinely *are* retired.
- **Related**: #3605 (`c43cb269`, the newest of the three in-frame callers),
  #2519, #917 (which established that this frame's history advances only on
  submit success — the swap the clear collides with).
- **Needs RenderDoc**: no.
- **Suggested Fix**: Document on `signal_temporal_discontinuity` that the
  `previous_rigid_models` clear is only meaningful to callers running outside
  `draw_frame`, and that in-frame callers must additionally set the `camera_cut`
  path (or move the clear to a flag the tail swap honours). No behavioural change
  needed today.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **TESTS**: A regression test pins this specific fix
