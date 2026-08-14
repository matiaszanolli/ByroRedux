# CON-D2-01 (HYPOTHESIS): build_tlas failure arm skips the frame's only AS_BUILD→ray-query barrier

- **Issue**: [#2931](https://github.com/matiaszanolli/ByroRedux/issues/2931)
- **Finding ID**: `CON-D2-01`
- **Labels**: `high,sync,vulkan,bug`
- **Source report**: [`docs/audits/AUDIT_CONCURRENCY_2026-08-14.md`](../../../docs/audits/AUDIT_CONCURRENCY_2026-08-14.md)
- **Run**: `/audit-suite rt-deep`, 2026-08-14, HEAD `205744ae`

> Immutable snapshot of the issue *as filed* (TD10-001 / #1156). GitHub is
> authoritative for current state — query `gh issue view 2931 --json state`.

---

> ### ⚠ HYPOTHESIS — do not ship a barrier change on this alone
>
> No Vulkan device, no captured validation run, and no RenderDoc capture backed this finding; it is source-read only. Per the project's standing rule against speculative Vulkan fixes, **confirm before changing any barrier, stage mask, or layout.**
>
> Confirm with a **release** build (debug is too slow to stream the dense cells that fault):
>
> ```
> BYRO_VALIDATION=1 cargo run --release -- ...
> BYRO_VALIDATION=gpuav cargo run --release -- ...
> ```
>
> The `--cornell` harness plus one dense cell adjudicates this. That is the same channel that confirmed #507945d8 at ~40 RAW hazards/frame pre-fix.


- **Severity**: HIGH — *filed as a **HYPOTHESIS** row per the speculative-fix guardrail; do not ship a barrier change on this reasoning alone.*
- **Dimension**: Compute → AS → Fragment Chains
- **Location**: `crates/renderer/src/vulkan/context/draw.rs` — `draw_frame`, the `if let Err(e) = accel.build_tlas(...)` arm and its `else` sibling; paired with `crates/renderer/src/vulkan/context/skinned_blas_refit.rs` — `record_skinned_blas_refit`, and `crates/renderer/src/vulkan/context/post_passes.rs` — `record_volumetrics_pass`
- **Status**: NEW
- **Description**:
  `record_skinned_blas_refit` runs immediately before the TLAS build in the same
  command buffer. Its terminal barrier is
  `ACCELERATION_STRUCTURE_BUILD_KHR / ACCELERATION_STRUCTURE_WRITE_KHR →
  ACCELERATION_STRUCTURE_BUILD_KHR / ACCELERATION_STRUCTURE_READ_KHR` — scoped
  deliberately to hand the refit results to the TLAS build, nothing further. The
  *only* barrier in `draw_frame` that publishes acceleration-structure writes to
  the ray-query consumers (`memory_barrier(..., ACCELERATION_STRUCTURE_BUILD_KHR,
  ACCELERATION_STRUCTURE_WRITE_KHR, FRAGMENT_SHADER | COMPUTE_SHADER,
  ACCELERATION_STRUCTURE_READ_KHR)`, the `#415` COMPUTE widening) lives inside
  the **success** branch of `build_tlas`.

  On the failure branch, `draw_frame` writes the stale handle via
  `scene_buffers.write_tlas`, clears `scene_buffers.tlas_written[frame]`, and
  calls `patch_camera_rt_flag(.., 0.0)` — but emits **no** AS barrier at all.
  This frame's per-entity skinned-BLAS refits (`refit_skinned_blas`, `src == dst`,
  in-place) and any same-`cmd` first-sight builds
  (`build_skinned_blas_batched_on_cmd`) are therefore never made available to
  `FRAGMENT_SHADER` / `COMPUTE_SHADER` acceleration-structure reads.

  `rt_flag = 0.0` mostly saves this: `triangle.frag`'s `rtEnabled = sceneFlags.x > 0.5`
  gates every one of its ray queries, and `caustic_splat.comp` early-outs on
  `if (sceneFlags.x < 0.5) return;`. **`volumetrics_inject.comp` has no such
  gate.** It declares `topLevelAS` at set 0 binding 2 and reaches
  `rayQueryInitializeEXT` through `traceShadowBinary`
  (`crates/renderer/shaders/include/shadow_common.glsl`), whose only guard is
  `mask == 0u || tMax <= tMin`. `record_volumetrics_pass` gates its dispatch on
  `accel.tlas_handle(frame)` being `Some` — and after a `build_tlas` failure it
  *is* `Some`, because `ensure_tlas_state`'s `#2673` allocate-then-swap commit
  point leaves `self.tlas[frame_index]` untouched on every early return. That
  stale TLAS still contains instances pointing at the same per-entity BLAS
  device addresses that were refit in-place earlier in this very command buffer.

  Net: on a `build_tlas`-failure frame with fog active, the volumetrics injection
  compute pass ray-queries acceleration structures whose writes from the same
  command buffer carry no memory dependency to `COMPUTE_SHADER` /
  `ACCELERATION_STRUCTURE_READ_KHR`.
- **Evidence**:
  - `crates/renderer/src/vulkan/acceleration/tlas.rs` — `build_tlas` has exactly
    two fallible statements, `ensure_tlas_state(...)?` and
    `tlas.instance_buffer.write_mapped(device, &instances)?`, both strictly
    *before* `self.accel_loader.cmd_build_acceleration_structures(...)`. So the
    failure path records no AS write of its own — which is precisely why the
    missing barrier is easy to read as harmless, and why the *earlier* refits are
    the exposed party.
  - `crates/renderer/src/vulkan/context/skinned_blas_refit.rs` — the closing
    `memory_barrier` in `record_skinned_blas_refit` is
    `AS_BUILD/AS_WRITE → AS_BUILD/AS_READ`; its comment states the intent
    plainly ("BLAS refit writes → TLAS build reads").
  - `crates/renderer/shaders/include/shadow_common.glsl` — `traceShadowBinary`
    guards only on `mask`/`tMax`, no scene/RT flag.
  - `crates/renderer/shaders/volumetrics_inject.comp` — declares `topLevelAS`
    and includes `include/shadow_common.glsl`; `VolumetricsParams` carries no
    RT-enable field.
  - `crates/renderer/src/vulkan/context/post_passes.rs` —
    `record_volumetrics_pass` dispatches whenever
    `(accel.tlas_handle(frame), cluster_cull-derived lights)` are both `Some`,
    with no consultation of `scene_buffers.tlas_written` or the camera `rt_flag`.
  - No barrier between the failed `build_tlas` and `record_volumetrics_pass`
    covers this: cluster cull's trailing barrier is
    `COMPUTE/SHADER_WRITE → FRAGMENT/SHADER_READ` (wrong src access scope for an
    AS write), and the bulk pre-render-pass barrier is `HOST_WRITE`-sourced.
- **Impact**:
  Read-after-write on acceleration-structure memory from a compute ray query.
  Practical blast radius on a failure frame: garbage volumetric shadow
  visibility (fog flicker / black froxels) at best; on drivers that fault on
  partially-written BVH traversal, a device-lost. Bounded to frames where
  `build_tlas` fails — i.e. VRAM exhaustion during a dense-cell TLAS grow — but
  those are exactly the frames already under stress, and the `#2673` /`#2674`
  work established that this warn-only failure path is a real, reachable state
  worth hardening rather than a theoretical one. The gap also widens silently
  the moment any RT consumer stops honouring `rt_flag`.
- **Trigger Conditions**:
  A single frame in which **all** of: (a) at least one skinned entity is drawn
  and its BLAS is refit or first-sight-built into the per-frame `cmd`;
  (b) `accel.build_tlas` returns `Err` (either `ensure_tlas_state` failing to
  allocate the TLAS buffer / AS / scratch, or `instance_buffer.write_mapped`
  failing); (c) `self.tlas[frame]` from a prior frame is still live, so
  `tlas_handle(frame)` is `Some` — guaranteed by `#2673`'s commit-point
  discipline; (d) volumetrics is not on the neutral-frame path, i.e.
  `fog_extinction_per_meter > 0` or `fog_volumes` non-empty, and `cluster_cull`
  is present. Requires no thread interleaving — it is a single-command-buffer
  GPU-stage reordering window.
- **Verification Path**:
  **Not observable in `cargo test`** (no headless device assertion reaches a
  barrier scope) and not visible in a normal validation run, because the trigger
  needs an allocation failure. Confirming signal, in order of cheapness:
  1. Add a temporary fault injection mirroring `BYRO_FSR_FORCE_DISPATCH_FAIL`
     (e.g. force `build_tlas` to return `Err` on one frame), run a fogged
     exterior with a skinned actor under `BYRO_VALIDATION=1` (release build,
     Synchronization Validation on via `instance.rs::validation_enabled`), and
     look for a `SYNC-HAZARD-READ-AFTER-WRITE` on the skinned BLAS backing
     buffer / acceleration structure at the `vkCmdDispatch` of the volumetrics
     injection pass. That message is the confirmation; its absence over a
     forced-failure run is the disproof.
  2. RenderDoc: capture a forced-failure frame and compare the resource state of
     a per-entity skinned BLAS between the `refit_skinned_blas` build command and
     the volumetrics inject dispatch — the absence of any intervening barrier
     touching AS memory is directly visible in the command list.
  3. Visible artifact class (weakest): fog/godray flicker or black froxel columns
     around skinned actors on the single frame after a TLAS-allocation warning
     (`"TLAS build failed: …"`) in the log.
- **Related**: #2673 (CONC-D1-NEW-01 — introduced the stale-handle + `rt_flag`
  clearing on this same failure arm, without an AS barrier), #2674
  (CONC-D1-NEW-02 — moved `build_tlas`'s bookkeeping commit past the recorded
  build for the same failure arm), #415 (the `COMPUTE_SHADER` dst widening on the
  success-arm barrier this finding says is unreachable on failure), #2403
  (CHAIN2-D2-01 — the sibling case where a chain relied on another pass's
  incidental trailing barrier), #1105. Adjacent to but distinct from the
  Dimension 1 AS-build/read-barrier sweep: the barrier in question is the
  *terminal link of the M29 skin chain*, and the exposure is a compute consumer
  in the post-pass sequence.
- **Suggested Fix** *(direction only — do not land without the confirmation
  above)*: hoist the `AS_BUILD/AS_WRITE → FRAGMENT|COMPUTE / AS_READ`
  `memory_barrier` out of `build_tlas`'s `else` arm so it is emitted on both
  arms whenever `record_skinned_blas_refit` recorded any AS write this frame
  (`skin_dispatch_ran` plus a non-empty refit/build set is the existing signal).
  A strictly-additive alternative that needs no barrier reasoning: make
  `record_volumetrics_pass` skip its dispatch (falling through to
  `record_neutral_frame`) when `scene_buffers.tlas_written[frame]` is `false`,
  which is exactly the "this frame's TLAS never landed" latch the failure arm
  already clears — that closes the only currently-reachable consumer without
  touching a stage mask.

---

**Summary**: 1 finding (1 HIGH, filed as HYPOTHESIS). 14 checklist guards
verified intact. 3 candidates dropped as already-filed (#2798, #2780, #2769).

---

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, the sibling BLAS/TLAS path)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **TESTS**: A regression test pins this specific fix

---

*Filed by `/audit-publish` from [`docs/audits/AUDIT_CONCURRENCY_2026-08-14.md`](docs/audits/AUDIT_CONCURRENCY_2026-08-14.md) — `/audit-suite rt-deep`, 2026-08-14, HEAD `205744ae`. Verified CONFIRMED against current code at publish time.*
