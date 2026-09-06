# #4007 — REN-2026-09-06-D13-01: `signal_temporal_discontinuity` has phase-dependent semantics — two of its five limbs are inert from the two `record_post_passes` call sites, including `#3605`'s new one, and nothing documents or guards the phase requirement

**Labels**: low, renderer, bug

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D13-01), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: TAA
- **Location**: `crates/renderer/src/vulkan/context/mod.rs` (`signal_temporal_discontinuity`); call sites `crates/renderer/src/vulkan/context/post_passes.rs` (`record_taa_pass`, `record_upscale_pass`); the end-of-frame swap in `crates/renderer/src/vulkan/context/draw.rs` (`draw_frame`, the `std::mem::swap(&mut self.previous_rigid_models, &mut current_rigid_models)` immediately after `mark_dispatch_completed`)
- **Status**: NEW — a gap in the fix `c43cb269` shipped, not a regression of it
- **Description**: `signal_temporal_discontinuity` has five limbs:
  `svgf_recovery_frames.max(frames)`, `taa.signal_history_reset()`,
  `fsr_temporal.signal_reset()`, `volumetrics.signal_history_reset()`, and
  `previous_rigid_models.clear()`. Its documented contract includes *"The first
  frame after a discontinuity must not encode object motion against transforms
  from the retired scene/camera history."* That contract holds only for callers
  that run **before** `build_and_upload_instances` — i.e. outside `draw_frame`
  (streaming / save / debug-load / app-step / resize) or at the `camera_cut`
  site inside `assemble_camera_and_lights`. Both `record_post_passes` callers
  run *after* it, and:

  1. **`previous_rigid_models.clear()` is unconditionally undone.** `draw_frame`
     ends with `std::mem::swap(&mut self.previous_rigid_models, &mut current_rigid_models)`,
     refilling the map with this frame's models. The clear performed at
     `record_taa_pass` / `record_upscale_pass` time is discarded before the next
     frame's `uses_rigid_motion_history` lookup ever reads it.
  2. **`fsr_temporal.signal_reset()` is phase-fragile.** `reset_pending` is read
     into `fsr_frame` back in `assemble_camera_and_lights`; a reset raised
     afterwards would be cleared by this same frame's `mark_dispatch_completed()`
     (reached via `take_submitted_dispatch()` at the tail of `draw_frame`)
     before the next frame reads it. Today this is saved only by an accident of
     which states can coexist — `#2519` only signals when the dispatch *failed*
     (so `dispatched_this_frame` is false and the reset survives), and `#3605`
     only fires in `UpscalerMode::Taa`, where `fsr_temporal` is `None`.

  Evaluating `#3605`'s call limb by limb: `taa.signal_history_reset()` is inert
  by construction (`taa_failed` has just latched, so `upload_params` and the
  dispatch are both gated off for the rest of the session);
  `fsr_temporal` is `None`; `previous_rigid_models.clear()` is undone per (1);
  `volumetrics.signal_history_reset()` **works** (it runs after
  `record_volumetrics_pass`, so clearing `dispatched_this_frame` correctly stops
  `mark_frame_completed` from validating the history); and the SVGF limb works
  only when the camera is moving (`REN-2026-09-06-D8-01`). Net delivered effect
  of `c43cb269` is one volumetrics history reset plus a conditional SVGF α bump.
- **Evidence**:
  - `signal_temporal_discontinuity`'s own comment: *"The first frame after a
    discontinuity must not encode object motion against transforms from the
    retired scene/camera history."*
  - Call order inside `record_post_passes`: `record_svgf_pass`,
    `record_caustic_splat_pass`, `record_volumetrics_pass`, `record_taa_pass`,
    `record_ssao_pass`, `record_composite_pass`, `record_bloom_pass`,
    `record_upscale_pass`, `record_presentation_pass`.
  - `previous_rigid_models` is read only at `build_and_upload_instances`
    (`self.previous_rigid_models.get(&draw_cmd.entity_id)`) and written only by
    the end-of-frame swap plus the `clear()` in question.
  - `record_taa_pass_signals_temporal_discontinuity_on_dispatch_failure`
    (`post_passes.rs`) asserts the *call* is present; nothing asserts which
    limbs of it survive to the next frame.
- **Impact**: No live visual defect today — both in-frame callers signal on a
  frame where the scene geometry did **not** change, which is exactly the case
  where correct (non-zeroed) motion vectors are wanted anyway. The defect is
  that a documented, load-bearing invariant is silently unenforceable from
  inside `record_post_passes`, and `#3605` has just established that call site
  as a normal place to signal from. The next in-frame caller that signals a
  *real* scene discontinuity gets a partial reset with no diagnostic.
- **Related**: `#3605` / `c43cb269`; `#2519` (the FSR sibling);
  `REN-2026-09-06-D8-01`.
- **Needs RenderDoc**: no.
- **Suggested Fix**: Either (a) make the in-frame limbs order-independent —
  have `previous_rigid_models.clear()` set a `suppress_rigid_history_next_frame`
  flag the next `build_and_upload_instances` consumes and clears, mirroring the
  existing `!camera_cut` guard in that same loop; or (b) document the phase
  requirement on `signal_temporal_discontinuity` and add a source-scan test in
  the style of `record_taa_pass_signals_temporal_discontinuity_on_dispatch_failure`
  pinning that no limb depends on being called pre-upload. (a) is preferable —
  a doc-only fix leaves the trap armed.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
