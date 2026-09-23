# #4780: CONC-D2-2026-09-23-01: The #3685 skip-clear latch also skips the temporal reset, so a skip on a latched slot leaves `history_valid` true

**Severity**: MEDIUM
**Labels**: medium, concurrency, sync, renderer, bug
**Source**: docs/audits/AUDIT_CONCURRENCY_2026-09-23.md (CONC-D2-2026-09-23-01)

- **Severity**: MEDIUM. The effect is visual temporal corruption, in the same class as denoiser ghosting.
- **Dimension**: Compute → AS → Fragment Chains (cross-frame ping-pong / history reuse)
- **Location**:
  - `crates/renderer/src/vulkan/context/post_passes.rs:568-828`: the `ran = false` arms at `:573` and `:815-819`, and the latch at `:823-828`.
  - `post_passes.rs:1425-1439` (`skip_clear_decision`).
  - `crates/renderer/src/vulkan/volumetrics.rs:1130` (`prev_camera_pos.w` ← `history_valid`), `:1399-1405` (`mark_frame_completed`) and `:1654-1659` (the resets that live only inside `record_neutral_frame`).
  - `crates/renderer/shaders/volumetrics_inject.comp:1503-1510`, `:2403` and `:2903` (history reads gated only on `prev_camera_pos.w`).
- **Status**: NEW. Commit `a02bf0374` (Fix #3685, 2026-09-03) introduced it. Before that fix, every skip frame called `record_neutral_frame`, which clears the integrated volume *and* resets `history_valid`, `last_simulation_time_seconds` and `combustion_active_until_seconds`. The latch now suppresses both halves on the second and later skips of a slot. #3685's own tests pin the clear count only.
- **Description**:
  - The injection history is ping-ponged per FIF slot (`previous = (frame + MAX - 1) % MAX`, `volumetrics/init.rs:558-592`). Reading it is valid only if the immediately preceding frame dispatched.
  - `history_valid` becomes true only in `mark_frame_completed` after a submitted dispatch. It becomes false only in `signal_history_reset` or in `record_neutral_frame`.
  - A skipped frame (`requires_dispatch` returns false, or the TLAS, cluster or geometry inputs are missing) writes nothing to its slot and calls `record_neutral_frame` only when `skip_clear_decision` says it is the first skip since that slot last ran.

  So in the sequence *dispatch (slot x, frame M) → skip (slot y, M+1) → dispatch (slot x, M+2)*:
  - Slot y's latch is still set from an earlier skip streak, so M+1 neither clears nor resets.
  - `history_valid` stays true from M.
  - At M+2 the inject reads `previousFroxel`, `previousEmissionHistory` and `previousCombustion{State,Dynamics,Optical}` from slot y. That slot was last written before the earlier streak, possibly many frames ago. The reprojection uses M+1's view-projection.
  - The combustion fields are simulation state, not a filter input. `transportCombustion` advects them (`inject.comp:1503-1510`, `:2403`), so stale smoke or fire state can come back.
  - Atmospheric fog blends that stale history at the 0.92 steady-state weight, bounded only by the density-rejection term.
- **Secondary trigger**: a `dispatch` error (`post_passes.rs:798-813`) is reported as `ran = true` and leaves `history_valid` true, but the slot was never written. That path is reachable only through a failed mapped write, which in practice does not happen.
- **Evidence**:
  - The `ran = false` arms (`post_passes.rs:573`, `:818`) fall through to `skip_clear_decision(ran, self.volumetrics_cleared_on_skip[frame])`. For a slot that is already latched, this returns `(false, true)`, and nothing touches `vol` on that frame.
  - `mark_frame_completed` is a no-op when `dispatched_this_frame` is false, so `history_valid` keeps its previous `true`.
- **Trigger Conditions**: `requires_dispatch` or the input availability toggles so that one frame dispatches between two skips, and the skipping slot is already latched. A realistic case is an interior with no fog ramp where `local_emitters_present` flickers because a radiating light enters and leaves the per-frame light list. The interior dust coefficient then turns `scatter_coef` on and off (`post_passes.rs:549-556`). Another case is a local fog volume at the frustum edge with no global medium.
- **Impact**: Stale or ghosted volumetric fog, and resurrected combustion transport state, after dispatch flicker. The artifact persists for roughly 12 or more frames at the 0.92 history weight. No GPU safety issue: every access is still correctly synchronized.
- **Verification Path**: Deterministic from code. A unit test can drive `skip_clear_decision`, `mark_frame_completed` and the `ran` sequence to assert that `history_valid` is false after any `ran = false` frame. No RenderDoc capture is needed.
- **Related**: #3685 (closed; the latch that introduced this), #2507 (the caustic precedent, which has no temporal state and so is unaffected).
- **Suggested Fix**: Split `record_neutral_frame` into its GPU clear, which stays latched, and a CPU-side temporal reset: `history_valid`, `dispatched_this_frame`, the simulation times, `combustion_active_until_seconds` and `combustion_light_grid_valid[frame]`. Run the reset on **every** `ran == false` frame, and ideally on the `Err` arm as well. The reset costs nothing on the GPU, so it keeps #3685's performance win.

**Source**: `docs/audits/AUDIT_CONCURRENCY_2026-09-23.md` (`/audit-suite --preset volumetrics-deep`, HEAD `2237da9c3`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related passes (other compute passes / history buffers / call sites)
- [ ] **TESTS**: A regression test pins this specific fix
