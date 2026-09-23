**HEAD**: `2237da9c3` · **Baseline**: `docs/audits/AUDIT_CONCURRENCY_2026-09-21.md` (@ `f97775ca8`) · **Audited**: Dims 1, 2, 3, 6, 7 (volumetrics slice only) · **Unchanged since baseline (skimmed)**: Dims 4 and 5 are out of suite scope. The core volumetrics sources have no commits since the baseline: `crates/renderer/src/vulkan/volumetrics.rs`, `crates/renderer/src/vulkan/volumetrics/`, and both `volumetrics_*.comp` shaders. The `/audit-suite` preset asked for a deep pass, so they were audited in full anyway.

# Concurrency & Synchronization Audit — 2026-09-23 (area scope: volumetrics / M55)

**Command**: `/audit-concurrency`, area-scoped to the volumetric fog system. It is one leg of `/audit-suite --preset volumetrics-deep`, run at depth `deep`.
**Severity scale**: `.claude/commands/_audit-severity.md`.

## Scope and method

- **Files in scope**
  - `crates/renderer/src/vulkan/volumetrics.rs`, `volumetrics/init.rs` and `volumetrics/noise.rs`.
  - The shaders `volumetrics_inject.comp` and `volumetrics_integrate.comp`.
  - Every site that records or drives these passes:
    - `context/post_passes.rs`: `record_volumetrics_pass` and the skip-clear latch.
    - `context/draw.rs`: order of the `draw_frame` steps, `mark_frame_completed`.
    - `context/assemble_camera_and_lights.rs`: the drain of the combustion moments.
    - `context/dispatch_skin_and_cluster.rs`: TLAS build and publish.
    - `context/resize.rs`, `context/teardown.rs`.
    - `composite.rs`, which binds the integrated volume on binding 6.
    - The CPU-side feed `byroredux/src/render/fog_volumes.rs`.
- **Dimensions not covered.** Dims 4 (scheduler proof) and 5 (physics RwLock) have no volumetrics surface and are out of suite scope.
- **How the work was done**
  - One auditor, no sub-agents. Scratch notes are in `/tmp/audit/concurrency/dim_{1..7}.md`.
  - No engine launch. No cargo build or test run; this audit is code reading only.
  - Each claim below cites file:line at HEAD and gives a concrete hazard scenario.
- **Dedup sources**
  - `/tmp/audit/concurrency/issues.json`: 4,660 issues, all states.
  - The `docs/audits/` concurrency, renderer and regression reports from 08-14 to 09-22. The volumetrics-titled issues checked were #1105, #1419, #1463, #2931, #2673, #3646/#3647, #3685, #3834, #3835 and #4535. All are closed and their fixes are in place.

## Summary

| Severity | NEW | Regression | Existing |
|---|---|---|---|
| CRITICAL | 1 | 0 | 0 |
| HIGH | 0 | 0 | 0 |
| MEDIUM | 1 | 0 | 0 |
| LOW | 0 | 0 | 0 |

| ID | Sev | Dim | Title |
|---|---|---|---|
| CONC-D1-2026-09-23-01 | CRITICAL | 1 | After a TLAS build fails, the volumetrics inject keeps tracing the stale per-slot TLAS past the BLAS deferred-destroy window, so a persistent failure can make it traverse freed BLAS |
| CONC-D2-2026-09-23-01 | MEDIUM | 2 | The #3685 skip-clear latch also skips the temporal reset: a skip on a latched slot leaves `history_valid` true, so the next dispatch reprojects froxel and combustion history that is stale by an arbitrary number of frames |

**Headline.** The barriers and layouts in the steady-state inject → integrate → composite chain are correct. Every hazard pair has a scope that covers it:
- The #3647 TRANSFER source scope is in place.
- The #2931 AS publish runs on both arms of the TLAS build.
- The #1105 latches are symmetric.
- All per-FIF resources, including the host-readback moment buffer, are indexed correctly.

Both findings are state-machine problems on degraded or skipped paths, not missing barriers:
1. The build-failure fallback outlives the BLAS lifetime contract.
2. The skip-clear latch dropped a CPU-side temporal reset that used to ride along with the clear.

---

## Findings

### CONC-D1-2026-09-23-01: After a TLAS build fails, the volumetrics inject keeps tracing the stale per-slot TLAS past the BLAS deferred-destroy window

- **Severity**: CRITICAL. The impact is a use-after-free of an acceleration structure: a ray query dereferences a freed BLAS, leading to a GPU page fault or `VK_ERROR_DEVICE_LOST`. The trigger is failure-path only. The rating follows the precedent of #2673, which was rated CRITICAL for the same VRAM-pressure regime.
- **Dimension**: Vulkan Queue & AS Sync
- **Location**:
  - `crates/renderer/src/vulkan/context/dispatch_skin_and_cluster.rs:324-361` (failure arm) and `:383-392` (this comment states that volumetrics still ray-queries).
  - `crates/renderer/src/vulkan/context/post_passes.rs:575-578` and `:608-621` (the gate is `tlas_handle(frame)` only).
  - `crates/renderer/src/vulkan/acceleration/tlas.rs:807-840` (allocate-then-swap keeps the old TLAS), `:57`, `:468` and `:548` (the LRU clock and the working set).
  - `crates/renderer/src/vulkan/acceleration/blas_static.rs:66` (`drop_blas`), `:314` and `:413` (the batched build bumps the clock and evicts), `:1252-1318` (`evict_unused_blas`).
  - `crates/renderer/src/vulkan/acceleration/static_working_set.rs:25-27`.
  - `crates/renderer/src/deferred_destroy.rs:46`.
- **Status**: NEW. This is a residual of the #2673 design (keep the old AS alive when a build fails) combined with #2931 (publish the barrier on both arms). Neither issue covers BLAS lifetime under a reused stale TLAS. #1449 fixed only the immediate-destroy case.
- **Description**: Since #2673, `ensure_tlas_state` is allocate-then-swap. When `build_tlas` fails, `tlas[frame]` keeps the TLAS from this slot's last *successful* build, and `tlas_handle(frame)` stays `Some`. The failure arm makes the main pass safe by clearing `rt_flag`. `caustic_splat.comp` is also safe: it exits early on the same `sceneFlags.x` (`caustic_splat.comp:316`). `volumetrics_inject.comp` has no such gate. `record_volumetrics_pass` checks only `accel.tlas_handle(frame)`, so every failure frame binds the stale TLAS and traces shadow rays through it. The code comment at `dispatch_skin_and_cluster.rs:383-392` says so explicitly.

  The BLAS lifetime contract does not account for a TLAS that old:
  - Eviction protects only the current frame's draw set. `build_tlas_instances` rebuilds `static_working_set` from this frame's draws (`tlas.rs:468`, `:548`), and this happens even on a frame whose build later fails. Beyond that it protects only by LRU age: `MIN_IDLE_FRAMES = MAX_FRAMES_IN_FLIGHT + 1` ticks of `frame_counter`.
  - `frame_counter` also advances on failed builds (`tlas.rs:57`), and once per streaming `build_blas_batched` call (`blas_static.rs:314`), which also evicts (`:413`).
  - Evicted BLAS and BLAS dropped at cell unload (`drop_blas`, `:66`) are freed after `DEFAULT_COUNTDOWN = MAX_FRAMES_IN_FLIGHT` ticks.
  - The countdown only guarantees that command buffers which *built* a TLAS referencing the BLAS have retired. A stale TLAS that is re-bound on a later failure frame re-introduces those references into a new command buffer.
- **Evidence**: In `record_volumetrics_pass`, the only TLAS gate is:
  ```rust
  let vol_tlas = self.accel_manager.as_ref().and_then(|accel| accel.tlas_handle(frame));
  ```
  It has no `tlas_build_succeeded_last_frame` or `rt_flag` term, although `tlas_build_succeeded_last_frame` is set to `false` at `dispatch_skin_and_cluster.rs:315` and to `true` only at `:409`.
- **Trigger Conditions** (FIF = 2):
  1. Slot *a* last built successfully at frame A. BLAS *X* is in that TLAS but is not drawn after A. For example, it belongs to a cell that is being streamed out, or it was culled.
  2. Slot *a*'s build then fails persistently. Plausible causes are TLAS growth failing under VRAM pressure, the exact regime #2673 describes, or a failing per-slot shrink/regrow. It keeps failing at A+2 and A+4.
  3. Meanwhile, streaming's `build_blas_batched` (which bumps the clock and evicts), or a cell-unload `drop_blas`, queues *X* for deferred destruction at about A+1.
  4. *X* is freed at the tick of about A+3.
  5. At A+4, slot *a*'s volumetrics dispatch traces the stale TLAS, which still references *X*.

  With no streaming, eviction by the other slot's successful build needs about three consecutive failures on slot *a*. BLAS eviction and TLAS allocation failure are both symptoms of VRAM exhaustion, so the two conditions tend to occur together.
- **Impact**: Device loss or a GPU page fault during a cell transition under VRAM pressure. At best, garbage shadow visibility in the fog. Only the volumetrics inject is exposed, because it is the one ray-query consumer without an RT-valid gate.
- **Verification Path**: Not reachable by `cargo test`. To reproduce:
  1. Add a temporary fault gate that makes `ensure_tlas_state` fail on one slot for about 10 frames.
  2. Set a tiny BLAS budget so that eviction fires.
  3. Stream a cell boundary under `BYRO_VALIDATION=gpuav`.

  The expected signal is a GPU-AV invalid acceleration-structure or device-address report, or `VK_ERROR_DEVICE_LOST`.
- **Related**: #2673 and #2931 (both closed; their fixes are the premise of this finding), #1449 (closed).
- **Suggested Fix**: Treat a failed build as "no TLAS" for the volumetrics dispatch. Gate `vol_tlas` on `self.tlas_build_succeeded_last_frame`, so the existing `ran = false` → neutral-clear path takes over, just as `rt_flag = 0` does for the fragment consumers.

  An alternative is to pin, per slot, the BLAS entries that the live TLAS still references. That means excluding `last_blas_addresses` from eviction and delaying `drop_blas` until that slot rebuilds successfully. The gate is smaller and matches the existing caustic and fragment policy.

### CONC-D2-2026-09-23-01: The #3685 skip-clear latch also skips the temporal reset, so a skip on a latched slot leaves `history_valid` true

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

---

## Verified clean (volumetrics slice)

- **Inject → integrate → composite barriers** (`volumetrics.rs`):
  - HOST→COMPUTE UBO barrier at `:1242-1251`. Defense-in-depth only (#4182).
  - Stage B history pairs at `:1263-1287`: READ→WRITE on this slot, WRITE→READ on the previous slot, for all five history families.
  - Stage D `inj_to_int` plus `pre_int_write` at `:1310-1336`. The source scope is COMPUTE|FRAGMENT|TRANSFER with SHADER_READ|TRANSFER_WRITE, which covers the prior composite read, the prior integration write (chained through Stage F) and a prior neutral clear (#3647). Guarded by `volumetrics_dispatch_names_transfer_in_its_source_scope` in `caustic.rs`.
  - Stage F publishes COMPUTE→FRAGMENT on `integrated[frame]` at `:1361-1370`.
  - Composite binds `integrated_views()[i]` per FIF in GENERAL (`composite.rs:814-817`, `:1177-1180`).
  - Only `composite.frag` samples the froxels. I grepped every shader to confirm this.
- **Neutral path.** `clear_general_accumulator` with consumers COMPUTE|FRAGMENT puts TRANSFER into the source scope structurally (`descriptors.rs:330-380`, `volumetrics.rs:1643-1652`).
- **Observation, not a finding.** After a previous frame that did not dispatch, the Stage B READ→WRITE edge on the four non-lighting history fields relies on the unrelated global COMPUTE/SHADER_WRITE→COMPUTE/SHADER_READ barrier at `post_passes.rs:651-658` for WAW availability. That barrier is recorded unconditionally right before every dispatch, so the edge is covered today. If that barrier is ever narrowed to buffer-only barriers, re-check this edge with `BYRO_VALIDATION`.
- **TLAS publish.** The AS_BUILD→FRAGMENT|COMPUTE AS_READ barrier runs on both build arms (`dispatch_skin_and_cluster.rs:397-405`, #2931), and the TLAS is per FIF. There is a WAR execution chain from the previous use of the slot to the rebuild: Stage F COMPUTE→HOST, then the TLAS instance barriers HOST→TRANSFER→AS_BUILD (`tlas.rs:199-251`).
- **Descriptor updates** for bindings 2, 3-5 and 19-21 are recorded after the all-slots fence wait and before the set is bound. The `tlas_written` / `lights_written` / `boundary_geometry_written` latches are set and reset symmetrically (#1105).
- **Frames-in-flight duplication.** All froxel families, the parameter / fog-volume / cluster / index / moment / integration-UBO buffers, and the descriptor sets are per FIF. The noise volumes are read-only after a fenced one-time upload (`volumetrics/init.rs:885-1027`). The global vertex/index SSBOs on bindings 20/21 are replaced through fenced one-time copies and deferred destroy, and are rebound on every dispatch.
- **Combustion moment readback.** The host drains after the all-slots wait (`assemble_camera_and_lights.rs:103-112`, which runs after `sync_and_acquire_frame`). Stage F COMPUTE→HOST is present, plus the #4602 tail edge. `latched_drain` keeps the dirty latch on failure (#4535). Host zero-writes happen before the next submit, so they are visible to the `atomicAdd`.
- **Temporal commit.** `mark_frame_completed` runs only after a successful `queue_submit` (`draw.rs:2296-2298`). A `draw_frame` error exits the app (`app_frame.rs:662`). An in-frame `signal_temporal_discontinuity` (camera cut, TAA failure) conservatively cancels promotion.
- **Resize and teardown.**
  - `device_wait_idle` runs before destroy and recreate (`resize.rs:60`, `:864-907`). A failed recreate destroys the unpublished pipeline and exits.
  - The composite binding-6 rebind is at `:1003-1050`; `volumetrics_cleared_on_skip` is reset at `:1248`.
  - Main framebuffers stay empty until finalize, so the #1211 guard (`draw.rs:1796`) blocks the `AboutToWait` frame that winit 0.30 still emits after `exit()`.
  - `destroy()` releases every owned object (`volumetrics.rs:1805-1893`).
- **CPU / ECS side.** `collect_fog_volumes` takes only shared reads (FogVolume, GlobalTransform, CombustionState, TotalTime) on the main thread in `build_render_data`, outside the scheduler. `fog.rs` does no ECS access. Volumetrics state is `&mut VulkanContext`-owned and uses no locks.
- **Allocator and queue.** No volumetrics path holds the allocator guard or the queue guard across a submit or a fence wait.

Suggested next step: `/audit-publish docs/audits/AUDIT_CONCURRENCY_2026-09-23.md`. Use domain label `sync` for CONC-D1-2026-09-23-01 and `renderer` for CONC-D2-2026-09-23-01.
