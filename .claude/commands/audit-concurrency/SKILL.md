---
description: "Audit Vulkan queue/AS sync, ECS lock ordering, scheduler access declarations, RwLock patterns, deadlock potential"
argument-hint: "--focus <dimensions> --depth shallow|deep"
---

# Concurrency and Synchronization Audit

Audit ByroRedux for data races, deadlocks, incorrect lock ordering, missing Vulkan synchronization and
thread-safety violations.

**Architecture**: Orchestrator. Each dimension runs as a Task agent (max 3 concurrent).

See `.claude/commands/_audit-common.md` for layout, methodology, dedup, finding format and the
path-reference convention; `.claude/commands/_audit-severity.md` for the scale. Rows that matter here:

| Condition | Minimum Severity |
|-----------|-----------------|
| Data race on Vulkan queue / use-after-free / AS built at wrong address | CRITICAL |
| Vulkan spec violation (missing barrier, fence misuse); missing AS barrier (build → shader read) | HIGH |
| Resource / descriptor / command-buffer leak per frame; missing cleanup on swapchain recreate | HIGH |
| ECS deadlock potential (RwLock ordering violation) | HIGH |
| FFI lifetime violation across the cxx bridge | CRITICAL |

Dimensions run by blast radius. Each has `Paths:` and `First step:`; skip a dimension whose Paths have
no commits since the last `AUDIT_CONCURRENCY_*` report.

## Parameters (from $ARGUMENTS)

- `--focus <dimensions>`: comma-separated dimension numbers (e.g. `1,3`). Default: all 7.
- `--depth shallow|deep`: `shallow` = barrier/lock presence; `deep` = trace concurrent paths and timing windows. Default `deep`.

## Extra Per-Finding Fields

- **Dimension**: Vulkan Queue & AS Sync | Compute → AS → Fragment Chains | ECS Lock Ordering | Scheduler Access Declarations | RwLock Patterns (Resource↔Storage, Physics) | Resource Lifecycle | Worker Threads
- **Trigger Conditions**: exact timing/concurrency window to reproduce.
- **Verification Path** (Vulkan-sync): `cargo test`, validation layer, or only RenderDoc.

## Speculative-fix guardrail (read before reporting any Vulkan-sync finding)

Barrier / stage-mask / layout / semaphore bugs are largely **invisible to `cargo test`**. Do not
propose a change on reasoning alone; frame it as **"needs validation-layer or RenderDoc confirmation"**
and name the confirming signal (a `VUID-*` message, a RenderDoc resource-state mismatch, a visible
artifact). "This barrier looks wrong" is a HYPOTHESIS row, not a fix.

- **Cheapest evidence**: `BYRO_VALIDATION=<v>` (`crates/renderer/src/vulkan/instance.rs`,
  `validation_enabled`) enables the Khronos layer + Synchronization Validation in a **release** build;
  `BYRO_VALIDATION=gpuav` adds GPU-assisted validation. Messages route into the Rust log. A
  sync-validation RAW/WAR/WAW hazard count is the confirmed-bug signal; capture before escalating past
  HYPOTHESIS.
- **A host fence wait is not a device edge.** `sync_and_acquire_frame.rs` waits on EVERY
  frame-in-flight fence, so frames never overlap on the queue — yet sync validation models the queue and
  only sees barriers (#4177/#4179/#4181/#4293). Do not dismiss a validation-reported hazard because "the
  fence covers it", and do not justify a barrier by "frames can overlap".
- HOST_WRITE→shader barriers are defense-in-depth, not a spec requirement (host writes before
  `queue_submit` are already visible, #4182) — an "unneeded" one is not a finding.

## Phase 1: Setup

1. Parse `$ARGUMENTS`. 2. `mkdir -p /tmp/audit/concurrency`. Toolchain: every `cargo test -p byroredux` / `--workspace`
   step below needs rustc >= 1.96 (wasmtime 49 / cranelift 0.136 MSRV since 15a6b1d2d; it was 1.94 under #4466; the repo pins 1.100.0, 3c9949ec9) —
   rustup cargo per `docs/contributing.md` § Toolchain note — or the bin crate gives no feedback.
3. `gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json number,title,state,labels > /tmp/audit/concurrency/issues.json`

## Phase 2: Launch Dimension Agents

### Dimension 1: Vulkan Queue & Acceleration-Structure Sync (CRITICAL surface)
Paths: `crates/renderer/src/vulkan/{sync.rs,texture.rs}`, `crates/renderer/src/vulkan/context/{init,init_stages,draw,sync_and_acquire_frame,resize,skinned_blas_refit}.rs`, `crates/renderer/src/vulkan/acceleration/`
First step: `git log --since=<last-report-date> --format='%h %s' -- crates/renderer/src/vulkan/{sync.rs,acceleration,context}`
**Checklist**:
- **Queue submission is single-Mutex.** `graphics_queue` / `present_queue` are `Arc<Mutex<vk::Queue>>`
  (created in `context/init.rs`; the per-pass construction arms moved to `context/init_stages.rs` by #6143, 3e6f6b66a; `present_queue` is an `Arc::clone` of the graphics queue when the
  families match, so one Mutex serialises submits + presents). Vulkan requires external synchronization
  (`VUID-vkQueueSubmit-queue-00893`): bind the `MutexGuard` and keep it across `queue_submit` /
  `queue_present` even though `vk::Queue` is `Copy` (a temporary `.lock().unwrap()` drops at end of
  statement — `draw.rs` documents this at the submit). Release before any `wait_for_fences`. Also check the
  one-time-command submit in `texture.rs` (`one_time_lock_scope_tests` pins lock → submit → unlock → wait, #1713;
  the queue acquire inside the fence window is `lock_recovering`, the fence lock still `expect`s by design and
  `VulkanContext::drop` recovers it — #5209). The helper's wait-failure arm frees its command buffer / fence only on
  `ERROR_DEVICE_LOST`; any other error leaves a possibly-pending submission alone and poisons the reusable-fence mutex
  via a caught panic (#5270, d194f7d60; pinned by `wait_failure_arm_disposes_only_on_device_loss`).
- **Frame-in-flight discipline (the both-slots wait).** `sync_and_acquire_frame.rs` waits on all
  `in_flight` fences before re-recording; that is the safety argument for the immediate scratch free in
  `build_skinned_blas_batched_on_cmd`, the TLAS resize, and every non-per-FIF resource. It is
  device-idle-equivalent only at `MAX_FRAMES_IN_FLIGHT == 2`. The wait is a known throughput cost (dominates GPU-bound
  frames), not a sync bug: narrowing it is tracked as #5365 (open; preconditions: per-FIF or deferred-destroy riders, the
  SVGF previous-slot G-buffer read's in-command-buffer barrier — #282 is closed on the wait, the barrier is still owed — and a
  `BYRO_VALIDATION=1` run on both upscaler modes; the rider list also names `linear_capture_staging` since M-LIGHT RLT-REF-007) — do not re-file it. Guard: the const-assert in `sync.rs`
  (#870), `the_all_slots_wait_argument_is_pinned` (the rider list; re-derive it in the same change that narrows the wait) and
  `frames_in_flight_contract_names_every_dependent_resource` (#3643) — confirm none
  is `#[ignore]`d and a FIF bump would fail them. `image_available[frame]` must not be reused while an
  acquire is pending (comment block at the acquire → submit window); `render_finished` is per swapchain
  image (`render_finished_is_sized_and_indexed_per_swapchain_image`).
- **AS build → read barriers.** BLAS/TLAS builds/refits read later by ray queries need
  `ACCELERATION_STRUCTURE_WRITE_KHR → ..._READ_KHR` before the fragment consumer: static
  (`blas_static.rs`), skinned refit (`blas_skinned.rs`; `record_scratch_serialize_barrier` dst is WRITE|READ,
  #1790; scratch-serialize before the static batch's first build, #4177; the refit publish lives in
  `context/skinned_blas_refit.rs`), TLAS (`tlas.rs`; BLAS writes published to the TLAS build at frame
  scope, #4179). Missing/wrong-stage = HIGH; CRITICAL if the AS was built at a wrong/stale address.
- **AS build INPUT access flag (commit 507945d8).** Inputs to a build (instance copy → TLAS in `tlas.rs`;
  skinned-vertex compute write → BLAS build in `skinned_blas_refit.rs`) use `SHADER_READ` at the
  `ACCELERATION_STRUCTURE_BUILD` stage, not `ACCELERATION_STRUCTURE_READ_KHR`. Confirm via a `BYRO_VALIDATION` run.
- **Deferred destruction vs in-flight reads.** BLAS entries route through `pending_destroy_blas`
  (commit a476b256), BLAS scratch through `pending_destroy_scratch` (#1782); the tick runs AFTER the fence
  wait (`sync_and_acquire_frame.rs`, alongside the mesh and texture ticks) and shutdown drains. Any new
  immediate `destroy_acceleration_structure` at an eviction site = CRITICAL UAF. The skinned-batch scratch
  grow is deliberately immediate (both-slots wait) — not a missed deferral (#3643).
- **Swapchain recreate.** `recreate_swapchain` (`context/resize.rs`) idles the device
  (`device_wait_idle`) before destroying swapchain-dependent resources. The TLAS-resize `device_wait_idle`
  (`tlas.rs`, #1390) is belt-and-suspenders behind the both-slots wait.
- **Blocking one-time submits.** BLAS initial builds and staging copies fence-wait; flag one inside the
  per-frame path. The overlay uploads (HUD via `write_rgba_inplace`, Ruffle UI via `update_rgba`) are no
  longer one: both queue into `texture_registry/dynamic_rgba.rs`, which `record_pending_rgba_uploads`
  (called from `context/begin_frame_recording.rs`) records into the frame's own command buffer through a
  per-FIF staging arena, with barriers ordering prior sampling before the copy; an update is consumed only
  when that frame's submit succeeds (`note_frame_submitted` → `submitted(slot)`). Guards:
  `rgba_updates_use_the_frame_command_buffer_and_keep_descriptors`,
  `only_the_submitted_recording_consumes_its_pixels`. The HUD's `texture_handles: [u32; 3]` rotation is now a
  driver choice, not hazard protection. Check the extent-change path (new image, old one deferred) and that
  the staging slot is not rewritten while its frame is in flight. M39 texture promotions ARE a per-frame blocking
  one-time submit by design: `VulkanContext::update_texture_streaming` (`context/telemetry.rs`, called before
  `draw_frame`) runs `flush_pending_promotions` through the same sub-batch fence-wait path as first uploads
  (`texture_registry/upload.rs`), bounded by `MAX_PROMOTION_UPLOAD_BYTES_PER_FRAME` and `MAX_RESIDENCY_CHANGES_PER_FRAME`
  (`texture_registry/budget.rs`). Check it takes the graphics-queue Mutex only around the submit, uses the persistent
  `transfer_fence`, and that the new images' descriptor writes reach the frame's slot at its `begin_frame`
  (23f4aeee3 replays the deferred write against the live view, not the one queued; since #6629, f754eee28, the same call also writes the slot's `first_level` into the per-FIF `textureFirstLevel` table at set 0 binding 2, immediate and replay alike, so a frame never pairs a view with another image's level — binding 2 is not `UPDATE_AFTER_BIND`, hence written before a set is first bound); a second submit site per frame is the finding.
**Output**: `/tmp/audit/concurrency/dim_1.md`

### Dimension 2: Compute → AS → Fragment Chains
Paths: `crates/renderer/src/vulkan/{skin_compute,svgf,taa,caustic,water_caustic,volumetrics,bloom,groundcover,sky_cube,material,shadow_filter,restir}.rs`, `crates/renderer/src/vulkan/{groundcover,volumetrics,sky_cube}/`, `crates/renderer/src/vulkan/context/{post_passes,dispatch_skin_and_cluster,skinned_blas_refit}.rs`
First step: `git log --since=<last-report-date> --format='%h %s' -- crates/renderer/src/vulkan/context/post_passes.rs crates/renderer/src/vulkan/*.rs`
**Checklist**:
- **Skin chain (M29).** Palette build (`skin_compute.rs`) → `COMPUTE_WRITE→SHADER_READ` → per-mesh skin output →
  BLAS refit reads it → fragment ray query. The raster path skins inline in `triangle.vert`, so a
  `VERTEX_INPUT` barrier is not needed until a raster-from-skinned-SSBO path appears. **Second
  consumer (#3582)**: `caustic_splat.comp` dereferences `skinnedVertexAddress` inline (an include-graph
  trace cannot see it), so the refit publish dst mask must carry `COMPUTE_SHADER`. Guard:
  `skin_publish_barrier_consumer_tests` in `context/skinned_blas_refit.rs` (source-scans `.comp` files for
  the deref and requires the bit) — confirm it still scans every `.comp`.
- **Frame-tail order.** composite → bloom → TAA → upscale, pinned by `taa_resolves_the_post_bloom_scene_tap`
  (`context/post_passes.rs`); the TAA output slot arrives in `GENERAL` and must be restored to it after the blit.
- **Cross-frame ping-pong.** SVGF / TAA history, caustic and water-caustic accumulators, volumetrics
  (`lighting_volumes` → `integrated_volumes`) read the previous frame's slot; verify per-FIF indexing.
- **Volumetrics (#1105).** inject → integrate (COMPUTE→COMPUTE) → `composite.frag` (COMPUTE_WRITE→FRAGMENT_READ);
  `tlas_written: [bool; MAX_FRAMES_IN_FLIGHT]` latch must be set before and reset after each gated
  `dispatch` (symmetric); callers gate on `VOLUMETRIC_OUTPUT_CONSUMED`.
- **Bloom (#931).** per-mip `SHADER_WRITE → SHADER_READ` image barriers on both pyramids (post-barrier
  on the just-written mip only); `up_mips[0]` completes before composite samples it. Caustic: CLEAR → COMPUTE → FRAGMENT.
- **Ground-cover / sky-bake compute** (pipeline semantics belong to `/audit-exterior`): sync correctness
  stays here. `groundcover/frame.rs::record_scatter` orders counter zero-fill → extrema seeds (TRANSFER→TRANSFER,
  #4293), scatter → publish incl. counter readback (#4181), and the interaction field's trailing barrier. The field's
  ping-pong (`field_write_half` / previous-half header) is a two-step latch: `record_scatter` only stages the flip in
  `pending_field_commit` when `record_interaction` actually recorded the dispatch (a no-TLAS skip or failed upload must
  not flip it, #6031, 21b9743e0; `a_frame_without_a_recorded_dispatch_does_not_advance_the_field`), and
  `promote_field_commit` commits it through `commit_field_state` only after `queue_submit` returns `Ok`, beside
  `promote_skip_clear_latches` (a tail `Err` that discards the command buffer must not mark a half the GPU never wrote as
  valid, #6291, 96ab07189; `the_groundcover_field_flip_is_promoted_after_submit_not_at_record_time`); the M-LIGHT linear
  capture follows the same shape: `linear_capture_copy_recorded` only stages `pending_copy`, and the frame count + armed
  readback commit through `discard_` / `promote_skip_clear_latches` after `queue_submit` returns `Ok` (#6811, 8f95465c4;
  `the_linear_capture_copy_is_promoted_after_submit_not_at_record_time`) — the screenshot and depth-capture twins still arm at record time;
  `sky_cube.rs::record_bake` runs mid-frame (`context/build_and_upload_instances.rs`) — check its
  publish barrier before the first consumer. Evidence is a `BYRO_VALIDATION` capture, not reading.
- **Passes new since 2026-10-15.** The direct-shadow filter (`shadow_filter.rs`, dispatched inside
  `record_svgf_pass`, or on the SVGF-absent fallback; both go through `record_shadow_filter`, which also brackets
  its own GPU timer pair since fc83608d3) reads the current frame's ReSTIR reservoir buffer that `triangle.frag` wrote (FRAGMENT_WRITE →
  COMPUTE_READ) and writes the correction composite samples (COMPUTE_WRITE → FRAGMENT_READ); its a-trous ping-pong
  needs a barrier per iteration. The SKYAL cloud-shadow bake (`sky_cube/cloud_shadow.rs`, recorded from
  `sky_cube.rs`) feeds scene set 1 binding 24 and a per-FIF host readback of the camera column, which needs its own
  SHADER_WRITE → HOST_READ dependency and must be read only after that slot's fence. Every host readback invalidates non-coherent memory through the one `buffer::invalidate_allocation` (screenshot, depth and linear capture, `GpuBuffer::invalidate_if_needed`; #6960, c19f345b1); `depth_and_linear_capture_readbacks_invalidate_before_mapping` (`context/resources.rs`, #7027, 091023a73) pins invalidate-before-map on the capture twins, so a new readback spelling its own invalidate (or none) is the regression.
- **Ground-cover model tier** (`groundcover_models.rs`, #4413): its compute writes `GpuInstance`s into the
  *tail of the main instance buffer* after `build_and_upload_instances` and before the geometry pass, which
  draws them with indexed-indirect — COMPUTE_WRITE must reach both `VERTEX_SHADER` (instance read) and
  `DRAW_INDIRECT` (args), and the host upload of the frame's own instances must not race the tail.
- **Exposure meter** (`exposure_meter.rs`): post-bloom composite → `exposure_meter.comp` → this frame's
  `ExposureResource` slot, sampled unconditionally by `presentation.frag` and the FSR dispatch — the slot
  write must be published before both consumers, in fixed mode too.
- **Radiance cache (RCG, not yet a pass).** `gpu_timers.rs` reserves the update / resolve timer brackets (queries 64-67; `QUERIES_PER_FRAME` = 72 since the terrain-patch brackets took 68-69 and the TSD-4.2 patch-BLAS bracket 70-71) and `GpuRayBudget` carries `cache_update_stride`, but no cache compute pass records yet (nothing outside `gpu_timers.rs` names `TimerPass::CacheUpdate` / `CacheResolve`); when the update-path trace and the resolve fold land they join this dimension as a compute → compute → fragment chain over the cache buffers, with the same per-FIF and publish-before-consumer checks.
- **Combustion local grid (#6533, d3e3acc7b).** `volumetrics/local_grid.rs` `record_frame` (the `volumetrics_local_sim.comp`
  atlas sim, Stage B2 of `VolumetricsPipeline::dispatch`) runs BEFORE the froxel injection, which reads this frame's atlas
  and writes the irradiance field; Stage F's `composite_barriers` (atlas state + optical + irradiance, COMPUTE → FRAGMENT)
  and `record_publish_bricks` publish them to composite's close-range fire march. The atlas / irradiance images
  are double-buffered per FIF slot like the froxel fields. Sim → inject visibility, and what a skipped / neutral frame
  leaves in the atlas, are for a `BYRO_VALIDATION` capture, not reading.
- **Terrain displacement patches (#7152 TSD-4.1, 84f04205f; #7221 TSD-4.2, ff339f658).** `terrain_patch.rs` `TerrainPatchPass::record`, called from
  `VulkanContext::record_terrain_patches` (`context/dispatch_skin_and_cluster.rs`; `draw.rs` calls it right after
  `upload_terrain_tiles_if_dirty` and BEFORE `dispatch_skin_and_cluster`, the module docs and SAFETY comment name that order since #7244, a75402b05),
  re-displaces each resident slot IN PLACE in reserved ranges of the one global vertex buffer (not per-FIF) with
  `terrain_patch.comp`. Its barrier pair is the compute half of the contract: before = TRANSFER write (readback zero-fill, geometry upload, tile copy) +
  the previous frame's VERTEX_INPUT / VERTEX / FRAGMENT / COMPUTE reads of the patch ranges → COMPUTE (no AS_BUILD source stage: the previous
  frame's patch-BLAS build read of the vertices is ordered only transitively, see rider 16); after = SHADER_WRITE →
  VERTEX_ATTRIBUTE_READ | SHADER_READ | HOST_READ at VERTEX_INPUT / VERTEX / FRAGMENT / COMPUTE / ACCELERATION_STRUCTURE_BUILD / HOST (AS-build geometry
  reads are SHADER_READ at the AS_BUILD stage, the 507945d8 rule). The per-FIF host readback
  (seam samples + largest displacement) is `harvest`ed one frame later behind that slot's fence with `invalidate_if_needed`, and
  `bound[frame]` re-writes the descriptor set only when the vertex / tile buffer or its generation changed. Stage 4.2 puts the patches in the TLAS:
  `record_terrain_patches` ends by calling `record_terrain_patch_blas`, which fills the manager's kept request list and calls
  `AccelerationManager::record_terrain_patch_blas` (`acceleration/blas_terrain_patch.rs`) — ONE batched
  `cmd_build_acceleration_structures` for every build and refit of the frame, over disjoint aligned regions of the set's own
  `scratch` buffer (so no per-BLAS serialize barrier), bracketed by an AS_BUILD write → AS_BUILD read|write barrier before and an
  AS write → AS_BUILD | FRAGMENT | COMPUTE read barrier after, before the TLAS build in `dispatch_skin_and_cluster`. A mesh refits (`UPDATE`, `src == dst`)
  while it shows the same square at the same pool addresses and builds otherwise (at most two builds a frame, a rebuild after 600
  refits); a deferred or failed patch is simply absent from `ready`, and `tlas.rs` skips a patch draw whose BLAS is not `ready_address` (no stale-BLAS
  instance; `plan_patch_blas_ops` is unit-tested). The patch meshes are skipped by the static-BLAS restore path. One `scratch`
  buffer serves both frame slots, and only a GROWN scratch is retired through `pending_destroy_scratch`; the in-place vertex rewrite, the AS refit and the
  shared scratch rest on the all-slots wait plus the barrier chain (`sync.rs` rider 16, #7245, cc5e64d4b; pinned by
  `frames_in_flight_contract_names_every_dependent_resource`), so re-derive them in the change that narrows the wait (#5365). The per-frame lists live on
  `PatchBlasFrame` (#7246, 6c293b8d6) and are `mem::take`n out of the set for the record and put back whatever the outcome. The pass's own per-frame lists persist the same way (#7263, 2725b3aea): `record` clears `pending[frame]` after `harvest` has read it, before any early return, so `blas_requests` sees only this frame's dispatches. Overlap with a validation capture
  is the evidence; the TSD-4.2 commit's own measurement was sync-validation-clean at the tundra and FNV positions.
- **MaterialBuffer SSBO.** upload is `HOST_WRITE → VERTEX/FRAGMENT_READ`, before draw recording; flag only if it moves into a mid-frame compute path.
- **Volumetrics skip path (#4780, fixed d213521ef).** The #3685 skip-clear latch owns only the GPU clear
  (`record_neutral_frame`), and it (with the caustic twin) is staged in `pending_*_cleared_on_skip` and promoted only
  after `queue_submit` succeeds — a tail `Err` that discards the command buffer must not leave it set (#5956, f025a4b2d;
  `skip_clear_latches_are_promoted_after_submit_not_at_record_time`). The temporal reset is `record_skipped_frame`, run on EVERY not-dispatched frame
  (incl. the dispatch `Err` arm). Guard: `every_skipped_frame_drops_the_temporal_history_not_just_the_first`.
  The combustion occupancy mask is host-zeroed per slot and rides the all-slots fence wait (`sync.rs` rider
  list, #5215); its device RAW rests on `record_volumetrics_pass`'s global COMPUTE barrier — narrowing that
  barrier is a needs-syncval change.
- Closed not-reproduced (#4989, not a fix): back-to-back narrowed palette dispatches into one SSBO with no
  barrier — zero SYNC-HAZARD on four games incl. a forced two-run plan. Re-raise only with a `BYRO_VALIDATION` hazard.
**Output**: `/tmp/audit/concurrency/dim_2.md`

### Dimension 3: ECS Lock Ordering & Deadlock (system level)
Paths: `crates/core/src/ecs/{world,lock_tracker}.rs`, `byroredux/src/systems/`, `byroredux/src/extensions/`, `.github/workflows/ci.yml`
First step: `BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux` and the same with `-p byroredux-scripting` — the binary's graph is where every recent cycle lived (#4982–#4984, #5025, #5371, #5372, #5535, #5954, #6101, #6208, #6209, #6561, #6618, #7063, #7093, the two pre-baseline questions of the 2026-11-22 ECS audit (fad27f4fe, f3c78ac2f, below), the door pair of 4288659fa, and the untracked `live_body` / `live_forms` pool-before-component cycle that kept the lane red from 98aa53db9 to 4c1d54cc0; core alone stays green) — then CI's `lock-order-check` form, `cargo test --workspace --no-fail-fast --exclude byroredux-ui`. A nonzero failure count is a hard regression; the lane was red for exactly that from the XCT door-fade test (98aa53db9, 2026-11-14) until 4c1d54cc0, so a red `lock-order-check` is a real inversion, not CI noise. Measured 2026-11-20 at 2d857fff2 (the door finding below was fixed afterwards): `-p byroredux-scripting --lib` is green (1472 passed; the `ActorValues` → `ActorValueNames` → `ActorValues` cycle in `obscript_effects::tests::actor_value_commands_translate_the_code_and_set_mod_damage_and_restore` was #7063, fixed in 4cad5d974 — `reset_health` in `obscript_actor_values.rs` now drops the `ActorValueNames` guard before `clear_actor_value_damage`), but `-p byroredux --bin byroredux` recorded a cycle, `Door` → `TwoStateActivator` → `Door`, and failed eight door tests (`systems::door::tests::*` plus `interaction::tests::door_prompts_and_motion_share_the_lock_order`): the `door.state` command (`commands/door.rs`, 180080de7) held a `world.get::<Door>` guard across `world.get::<TwoStateActivator>`, the reverse of the sorted `query_2_mut::<TwoStateActivator, Door>` in `door_motion_system`. Regression guard: fixed in 04d1e99ae (#7093, the command now reads the activator, lock and form ID first and `Door` last), pinned by `door_state_and_door_motion_share_the_lock_order` (`commands/door.rs`, fails alone under the detector against the old order) — the lane is expected green on the bin again, but this sync did not re-run it
Machinery — TypeId-sorted pairs, tracker-scope arming, `lock_tracker` internals (check-before-insert,
`GRAPH` poison recovery, recursive-read warning) and poison resolution — is `/audit-ecs` Dim 1; do not
re-audit it here. This dimension owns how *systems* use it.
**Checklist**:
- **Static proof first.** For the parallel batch, `undeclared_parallel_count() == 0` +
  `unknown_pair_count() == 0` + `known_conflict_count() == 0` means no cross-thread blocking edge, hence
  no ABBA among declared parallel systems. Enforced in every build by `install_runtime_registries`
  (release `assert_eq!`) and in tests by `build_scheduler_reports_zero_access_conflicts` and
  `scheduler_access_invariants_hold_on_the_real_schedule`. Declaration completeness is
  `/audit-ecs` Dim 5's guard; its blind spots are Dim 4 below.
- **Dynamic supplement, reachability-bounded.** `BYRO_LOCK_ORDER_CHECK=1` (debug-only, opt-in graph)
  covers what declarations cannot: exclusive/cross-stage paths and hand-ordered N-lock holds. CI runs it
  twice — `lock-order-check` (hand-built worlds; since #4993 it takes cargo's own status via PIPESTATUS, so any
  test failure reddens it, pinned by `lock_order_job_propagates_cargo_status_through_tee`) and
  `vulkan-validation` (the only job where rayon dispatches the real parallel batch against a real world;
  pinned by `vulkan_validation_job_enables_the_lock_order_detector`, `vulkan_validation_job_fails_on_a_panic`
  and `vulkan_validation_job_resolves_lavapipe_and_fails_on_init_failure` in
  `byroredux/src/scheduler_access_tests.rs`). Since #4987 (6d5d8fa5f) the lane hard-fails unless the bench
  log carries the `Selected GPU:` line (`vulkan_validation_job_requires_a_selected_device`), so a green run
  did reach a device, and since #5261 (59115f54c) on an `rt-integrity: … rt_flag=1 … tlas_build=1` line, so the RT
  consumers ran (`vulkan_validation_job_requires_live_rt`, #5416) — but it drives a 5-frame `--bench-frames` run of
  the default no-game-data scene, and a green run proves only what it exercised. Its `[Vulkan]` grep is
  severity-blind, so the log lift is scoped to `byroredux_renderer::vulkan::device` (#5263, a88100975; asserted in
  `vulkan_validation_job_requires_a_selected_device`): the messenger stays at the error floor, and a red `[Vulkan]`
  run is an ERROR, not a WARN-level performance warning. A crate-wide `byroredux_renderer=info` lift is the regression.
- **The graph cannot tell `&mut World` from `&World`.** A function holding several read guards at once under
  `&mut World` cannot deadlock but still records edges and reddens the lane (#4982, the unload capture
  passes). The fix is the same snapshot-then-acquire shape, not an exemption.
- **A new system body is the usual source of the next cycle (#5535, 1cb12fb3d).** The lane went red on five cycles across
  26 tests, three of them production inversions in just-landed code: `eat_sleep`'s batched gather took
  `GlobalTransform → WalkSpeed → Transform` (every other mover takes `Transform` first), hair head-mount's
  `common_shape_rotation` held `MeshHandle` + `Children` across `bind_transform_relative_to`'s `Transform → Parent`, and the
  #5501 seated restore held `Furniture` across a `GlobalTransform` read (sandbox takes `GlobalTransform → Furniture`). The
  other two were test bodies holding a guard across a second read (#5032 shape, fixed in 3bc60e0be). A refactor can
  re-open a closed one: #5713 routed `common_shape_rotation`'s walk through `collect_subtree` (which takes `Children`
  internally) while the `MeshHandle` guard was alive, re-recording the edge #5535 removed (#5954, 7a4f6c1a2) — a shared
  helper that acquires internally counts as an acquisition at the call site. The same shape keeps recurring in new AI/idle
  systems: `idle_clip_playback_system` / `idle_resume_system` took `AnimationClipRegistry` under their `AnimationPlayer` guard
  while `animation_system_inner` holds the registry outermost (#6208, 1e37a7536 — resolve clip facts under one registry
  read, drop it, then take the player guard); the unload walk held `ParticleEmitter` across a `TextureHandle` read (#6209,
  a49073cc9); `material_rows` took `Material` then `MeshHandle` as two independent read queries (#6618, 2b9571e02);
  `improve_plan` held the player's `Inventory` guard while `stack_quality` took `ItemInstancePool`, the reverse of the save
  path's pool-then-Inventory order (#6561, 3c9dca757); `SetRelationshipRank` raised `CHRR` inside the quest-stage /
  objective pair, so the raise now queues on `DeferredFragmentEffects` and fires at the flush (#6572, 076de275e);
  `select_info` held `QuestStageState` across `evaluate()`, a recursive read behind a possibly-parked writer (#6615,
  6af13ef37 — the tracker now prints the first holder's stack under `RUST_BACKTRACE`, and skips the benign
  `CommandRegistry` re-read in `help`, #6617 bcab15f47); the door interaction prompt read `Door` under its `TwoStateActivator` query while `door_motion_system` takes the pair sorted (#7059, 4288659fa: snapshot the activators first; pinned by `door_prompts_and_motion_share_the_lock_order`); `find_package` held a `SceneAliasCandidate` query across `is_pickup_target` (→ `InventoryCatalog`) and
  `flee_package` read `AiCombatState` under `Transform` (ce22fcbcf — read cross-lock state before taking the guard); the actor-graph lazy clip loader read a registered clip's sync points under `StringPool` and then took `AnimationClipRegistry`, against `start_cli_animation` (registry, then pool; fad27f4fe — `registered_sync_points` takes the registry first, pinned by `the_sync_point_lookup_takes_the_registry_before_the_pool`); `collect_groundcover_disturbers` read each actor's `CharacterController` under `ActorValues` + `GlobalTransform`, closing the #3260 cycle against `character_controller_system` / propagation on every disturber-collecting frame (f3c78ac2f — it now records the candidate entities in `GroundCoverCollectScratch`, drops both guards, then reads the capsule radii under `CharacterController` alone; pinned by `the_disturber_gather_reads_controllers_after_dropping_global_transform`); `live_body` / `live_forms`
  (`cell_loader/relocated.rs`, reached from the door-fade `fade_in`) took the `FormIdPool` resource before the
  `FormIdComponent` query, closing `FormIdComponent → FormIdPool → FormIdComponent` against every other holder of the pair
  (4c1d54cc0: query first, then pool — the process-wide order); and the first ECS concurrency audit since 08-12 (dfcbcafd5, 2026-11-15)
  closed five more, all main-thread (so none could deadlock, but each panics the detector on a real world): `eat_sleep`'s diner gather
  took `GlobalTransform` before `WalkSpeed` while escort / follow / flee / travel / guard hold `WalkSpeed` across a `GlobalTransform` read
  (`the_diner_gather_takes_walk_speed_before_global_transform`), `submersion_system` kept a `WaterBedField` guard across the `WaterVolume` /
  `ParticleEmitter` write (`submersion_with_a_bed_field_takes_its_locks_in_one_order`), `kf_idle_work_pending` held `ActorCinematicState`
  over an `IdleClipCatalog` read (the reverse of `idle_clip_playback_system`), the SDK capture (`extensions/capture.rs`) held `Inventory`
  before `InventoryCatalog` (the reverse of `encumbrance_system`), and the unload walk read `AnimatedTextureFlip` after
  `MaterialTextureHandles` / `TerrainTileSlot` (now its own pass, as #6209). The sandbox seat pass read `CreatureActor` last, after `GlobalTransform` / `Furniture` / `Seated`, while other code holds a `CreatureActor` read across `ActorValues` then `GlobalTransform`; `a_creature_actor_never_takes_furniture` aborted the ABBA lane until ace99c164 took `CreatureActor` first (`sandbox_seat_system_inner`, `systems/sandbox.rs`). Attribution recipe: a temporary edge logger
  in `record_and_check`, `--test-threads=1 --nocapture`; fix by gathering under one guard, dropping it, then composing.
- **`FormIdPool`'s entity index is a leaf mutex (#6338, ab3a604bf).** `FormIdPool::entity_with_local` rebuilds a lazily
  cached `local → entity` map under an interior `Mutex` keyed on `FormIdComponent`'s structural generation + pool length;
  `resolve_entity_by_global_form_id` (`crates/scripting/src/condition/eval.rs`) calls it per actor per frame with the
  `FormIdComponent` read guard and `FormIdPool` read guard both held (storage first, then pool — the order its comment
  records). Nothing may be acquired while that mutex is held, and a new caller must take the pair in the same order (`live_body` / `live_forms` did not until 4c1d54cc0 — the pair is
  the commonest reverse-order site, since the pool reads like a plain lookup table).
- **Two resources held nested take them TypeId-sorted** (`resource_2_mut`, or its read twin `try_resource_2`), never two
  `try_resource` calls in code order: TypeIds shift with the unified feature set, so a code-order pair agrees with the
  sorted sites in one build and closes an ABBA in another — deterministic per build configuration, so it looks
  transient (#6101, 824c620a5: the quest stage / objective pair in `evaluate_guard_guard_free`, red only in the
  multi-crate `-p byroredux-core -p byroredux-physics -p byroredux-scripting --lib` lane; pinned by
  `try_resource_2_does_not_deadlock_against_resource_2_mut` in `crates/core/src/ecs/world_tests.rs`, which carries
  `#[cfg_attr(miri, ignore)]` — a lock-order test, not pointer provenance, #6234 5dc642ee7). The graph is process-wide, so a
  core test holding two fixtures in plain code order closes a cycle against any sorted user of the same pair, intermittently
  (#6808 b88ff14da `resource_2_mut_both_writable`; #6809 80b2a7d56 gave the plain-order query tests private `PlainA` / `PlainB`
  fixtures — never hold the shared `Health` / `Position` pair in plain order).
- **A fixture records only the edges its storages allow.** #5372 (1c15a2270) closed
  `LoadedCellIndex → StoryEvent → LoadedCellIndex` (the hello location resolved under the `StoryEvent` guard), which the
  lane never saw because the dialogue fixture lacked the Story Manager storages and `raise_hello_story_event` returned
  at its first line. A test world that skips the boot's registrations hides that chain from the detector, so check it (the disturber cycle above sat unseen because no fixture gave a disturber a `CharacterController`; the sync-point cycle because no test process ran both orders together).
- **Canonical order.** `docs/engine/ecs.md` § Lock-ordering policy is the arbiter for hand-ordered
  holds (`StringPool` is a sink: acquired last, nothing beneath it). New multi-lock code follows it.
- **Guard lifetime in system bodies.** No `query_mut` / `resource_mut` guard held across a call that
  re-enters the same storage/resource or acquires a pair the other way; nested patterns drop the first guard or
  use `query_2_mut`; no structural mutation (`World::insert` / `spawn` need `&mut self`) inside a system.
- **Guard ↔ sandbox boundary.** `byroredux/src/extensions/` hosts untrusted WASM behind an
  `Arc<Mutex<ExtensionHost>>` (`ExtensionHostSlot`, `extensions/systems.rs`). Every dispatch path snapshots
  ECS state before guest entry, drops every ECS guard, enters the guest, and commits its returned command
  batch afterwards (`extensions/dispatch.rs` module doc; `commands.rs::enter_guest`). Since #5592 (91a28a4c9) the
  eight `dispatch_*` families share one `ExtensionHost::deliver` body (grant + status gate, guest entry, log drain,
  commit), so audit that body once and the per-family select closures for what they capture; on the system side the
  same eight share `with_extension_host` (`extensions/systems.rs`, #5810 78cfa46df: acquire → lock → dispatch → apply
  commands → drop → emit, with the SKSE mod-event queueing collected under the lock and queued only after it drops). A host `Mutex`
  guard or ECS guard held across guest entry — or a host function that re-acquires ECS storage the
  caller already holds — is a deadlock/reentrancy finding. Trust-boundary questions are `/audit-safety` Dim 8.
**Output**: `/tmp/audit/concurrency/dim_3.md`

### Dimension 4: Scheduler Proof Soundness (regression guard)
Paths: `byroredux/src/boot/schedule/`, `byroredux/src/boot/registries.rs`, `byroredux/src/scheduler_access_tests.rs`, `crates/core/src/ecs/{scheduler,access}.rs`
First step: `cargo test -p byroredux -- scheduler_access system_access_declaration`
The access model and the mechanical declaration guard (`system_access_declaration_tests`,
`PARALLEL_SYSTEMS`) are owned by `/audit-ecs` Dim 5 — one guard line here, then aim at what it cannot see.
`sys.accesses` (`byroredux/src/commands/world_info.rs`) is the operator view of the same report.
**Checklist**:
- **Confirm the proof is live**: the two tests above are not `#[ignore]`d, the report's non-vacuity floors
  (≥9 parallel systems, ≥7 pairs) still hold, and `PARALLEL_SYSTEMS.len()` equals the `add_to_with_access` registration count (`the_parallel_system_table_covers_every_parallel_registration`). Registration lives in `byroredux/src/boot/schedule/`: `register_update_systems` is five contiguous order segments called in file order (#6359, 4ab5766e3), and since #5802's wave the segment functions, `register_post_update_systems` and `register_late_systems` are flat lists of calls into one file per subsystem (`update/<domain>/`, `post_update/`, `late/` — #6703 22e0f71fc, #6704 49aa54e36; the Late hub pin fell 858 → 16), each hub pinned by `per_frame_driver_functions_stay_within_their_size_pins` (code lines only, #6720). Order that file position used to carry is now a named constraint beside the later system: `Scheduler::run_before` / `run_after`, applied by `resolve_order` (#6702, 065aac9c4; an unknown name, a forbidden cross-stage/phase edge or a cycle panics at `build_scheduler`). A moved system keeps its declared access (`Access::with` lets a subsystem export its own declarations, #6701), so after any such move diff `system_names()` / `access_report()`. The ordering guards in `boot/schedule/mod.rs` (`schedule_order`) read the BUILT schedule, not registration text (#6684, 2a02c5446): `runs_before` counts two parallel systems in one stage as unordered, and a renamed or env-gated-off name panics rather than passing (`BYRO_NO_AI_LOCOMOTION` drops the AI movers). The kill-switch / timing pins stay text scans because they cover env-gated source branches.
- **Blind spots of the guard** (audit by hand): since #6685 (c98a4552f) the scan is a `syn` syntax-tree walk
  (`byroredux/src/acquisition_scan.rs`, test-only; `syn` is a dev-dependency of the bin) — comments, strings and
  `#[cfg(test)]` items cannot match, a helper's acquisitions are followed through the call (depth-limited), a
  function-pointer spelling counts, macro arguments are parsed as expressions with a token-walk fallback, and the
  declaration side reads the registration's access expression (following helpers like `bridged_script_getter_reads` and
  `.with(delegate)` paths). It still reads turbofish `query` / `query_mut` / `resource` / `resource_mut` (+ `try_`),
  `world.get` / `get_mut` / `has`, `PlayerEntity::get`, `drain_into_resource` (#6707) and generic `remove_component` /
  `insert_component` (#4994, #4821) — not `query_2_mut`, `resource_2_mut` or inferred types; it follows calls
  within a listed file but cross-file hops only where `PARALLEL_SYSTEMS` lists them (#4994,
  `cross_file_hops_and_get_forms_reach_their_acquisitions`); closures; exclusive systems (fourteen have their own
  `*_declares_everything_it_acquires` scan — `npc_combat_ai_system`, `interaction_system`, `container_loot_system` and `combat_damage_system` (#6546,
  5129240c4: M43 made their rows under-declare; a new read in one of these must reach its row in the same change — `supports_rest`'s `CharacterRuleset` read is declared on `interaction_system` since #7212, 188bb4535, and `combat_damage_system`'s #7195 unarmed-creature rule (2c5c264dc) adds `CreatureActor` / `EquippedWeapon` reads its row already declared; #7318, 40b2cda36, listed its kill-branch hops `raise_kill_story_event` / `note_death` / `entity_global_form_id` / `mod_misc_stat` / `challenges::note` and the `interaction_system` hops `jail::player_in_jail` / `supports_rest`, whose rows had missed `KillRecords`, `MiscStats`, `ChallengeState`, `OnDeathEvent` and the cell contexts), `papyrus_provider_system` (`papyrus_activations` is a second scan root, so its `NativeActivation` read is seen, #7311 2d62dccad), `legacy_obscript_load_order_system`,
  `equipment_appearance_system` (its mid-life import surface, #5069), and
  `npc_dialogue_selection_system`, whose scan follows the spoken-INFO fragment path into the scripting crate, #5307 (a spoken
  `EquipItem` of a weapon queues on `PendingNonArmorEquips`, undeclared until 3890897e3),
  `apply_selection`'s voice / AHEL hops `play_line_voice` + `raise_hello_story_event`, #5414 9d8fd4cc9, and the condition
  evaluator `evaluate_function` — 18 acquisitions incl. the `StoryManagerRng` write, #5649 197a9e245, and the
  deferred-effects flush `apply_at_depth`, which the old over-reading text scan reached by accident and the syntax walk
  lists explicitly, exposing 13 undeclared types now carried by `fragment_effect_family_access`, #6685;
  others only by the non-empty-access tests, e.g. `footstep_declares_its_physics_reads`, #5556; #7319, 623fccb8c, added the six declared exclusives that had none — `detection_tick_system`, `sneak_stance_system`, `player_cast_system`, `combat_input_system`, `player_bow_system` (the two spenders declare `remove_items`'s equipment / alias / scene claims) and `player_seat_system` — each had under-declared except the last; `bed_use_system` and `jail_bed_system` stay bare `add_exclusive`s with no scan, as does a hop an unlisted cross-file callee reaches, e.g. `interaction_system`'s perk-activate `run_activate_choice`, whose result scripts have no enumerable footprint). The table and these scans live in `system_access_declaration_tests`
  (`byroredux/src/boot/schedule/mod.rs`; `PARALLEL_SYSTEMS.len()` is checked against `parallel_registration_count`, a method-call count).
  An under-declared parallel system makes `known_conflict_count() == 0` unsound (the same-session
  `fly_camera_system` `GlobalTransform` write is the precedent, commit ac1d44f5c; #5641, 17a84f442, was the drowning
  death in `player_controller_system` raising the KILL story event without declaring `StoryEvent` + the cell-context
  reads — now a `raise_kill_story_event` hop in `PARALLEL_SYSTEMS`; the cross-crate hop behind it, the scripting crate's
  `note_death` — a `KillRecords` write plus `SceneAliasCandidate` / `CreatureActor` / `FormIdComponent` / `FormIdPool` /
  `PapyrusPlayerEntity` reads — is a listed hop too, #6217 ef48dbd03; a new CTDA arm's reads must be declared on
  `npc_dialogue_selection` the same way, 561541d6c; `player_controller_system` reached `LimbEffects` through `limbs::locomotion_mult` undeclared
  until dfcbcafd5, which declared it and made `limbs.rs` a listed cross-file hop). All three story-event
  producers (CLOC, AHEL, KILL) go through `raise_story_event` since #5647 (eca0c4709): the first event on an entity takes the
  `StoryEvent` marker, a second for the same entity appends to `PendingScriptStoryEvents` (marker guard released first, so the
  two never nest). `raise_story_event` lives in the scripting crate and has no turbofish at the call site, so #6447 (1ebb38bce)
  added it as a listed cross-file hop (`STORY_MANAGER_SRC`) and `player_controller_system`'s row now declares
  `writes_resource::<PendingScriptStoryEvents>()` — verified red without the declaration; the other rows reaching
  `raise_kill_story_event` are exclusives. Regression guard: any new parallel producer of a story event must declare the queue write.
- **Cross-stage sequencing is invisible to the analyzer** (`analyze_pair` reasons within one stage). A
  consumer in an earlier stage than its single producer silently reads last frame's value while every KPI
  stays green. Pinned today: `player_wind_read_is_declared_and_weather_writer_is_exclusive` (WindField:
  weather is an Early exclusive registered after the parallel player controller, so the controller reads
  the previous frame's wind by design, #3111/#4186), `billboard_runs_after_camera_follow_in_late`,
  `footstep_runs_after_camera_follow_in_late`, `submersion_runs_after_camera_follow_and_before_water_audio`
  (#3652/#3180/#4185 — each was a real one-frame-stale bug; the water-interaction bridge → `water_audio_system` edge that registration order alone carried is a named `run_after` and asserted by it since #7280, e2a4c54cc), `door_motion_runs_before_the_activator_that_drains_its_batch` (#7321, af9c6f55d: swapped, the activator drains every `TwoStateTransitionBatch` before a door sees it and no door animates; the same commit adds `door_motion_system`, `player_seat_system`, `bed_use_system` and `jail_bed_system` to `activation_flush_is_scheduled_before_every_activate_event_consumer`), `player_body_facing_runs_in_update_before_propagation`
  (#4995), `story_dispatch_and_forcegreet_update_ordering_is_pinned` (#5415, a9f94455d: CLOC producer → Story
  Manager dispatch → `quest_alias_refresh_system`, and `ambient_ai_package_system` → `forcegreet_system` → Late
  dialogue selection), `hit_event_consumers_run_after_every_producer` (ee6468fd1: `HitEvent` is a one-frame marker
  `event_cleanup_system` drains in Late, so `combat_damage_system` / `hit_event_system` registered ahead of the
  projectile-flight and VM `ProcessTrapHit` producers dropped every dart-trap hit; a registration-order bug the
  access analyzer cannot see; the ObScript `obscript_hit_system` consumer joined the same test as a split-out system, #6293
  6208348f7; #6813 listed `vats_mode_system` as a producer, but `vats_confirm` runs only from the `vats.confirm` console
  command in the debug server's Late drain after `event_cleanup_system`, pinned by
  `console_raised_hits_are_queued_after_the_frame_cleanup`, #7320), `vats_mode_is_written_after_the_early_movement_gate_and_before_its_action_consumers`
  (`VatsState`: the Early parallel `player_controller_system` movement gate and the app loop's `dt` scaling read the PREVIOUS
  frame's mode, so the world freeze trails the enter/exit edge by one frame by design; the writer is an Early exclusive that must
  stay after the `ActionState` edges and before `combat_input_system` / `interaction_system`, #6812), and the spoken-INFO path in Stage::Late must not claim the fragment journal cursor
  its same-frame `QuestStageAdvancedBatch` drains unread (#5297, 68a06509a). For any NEW single-writer / multi-reader resource,
  check writer-stage ≤ reader-stage and that a test like these pins it; fix by moving the *consumer* to
  a Late exclusive after the writer, not by moving the writer.
- **Exclusives** run serially after the parallel batch and are never paired; undeclared ones are by design (a
  system demoted to exclusive to clear a conflict stays exclusive). Flag only a *parallel* system that lost its declaration.
**Output**: `/tmp/audit/concurrency/dim_4.md`

### Dimension 5: RwLock Patterns — Resource↔Storage & Physics Step
Paths: `crates/physics/src/{sync,components,config}.rs`, `crates/physics/src/world/`, `byroredux/src/cell_loader/unload.rs`, `byroredux/src/systems/{character,forcegreet,eat_sleep}.rs`, `byroredux/src/ragdoll.rs`
First step: `cargo test -p byroredux-physics sync` and `BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux-physics`
**Checklist**:
- **TypeId sorting does not cover Resource↔Storage.** A `resource_mut` and a `query`/`query_mut` are an
  unordered pair, so no `resource_mut::<PhysicsWorld>()` guard may span a storage-query iteration or
  vice versa. `physics_sync_system` phases (numbered in its own comments): 1 collect newcomers +
  register, 2 push kinematic, 2.5 water buoyancy (`crate::water::apply_buoyancy`), 3 Rapier step, 4 pull
  dynamic. Phase 1 `collect_newcomers` collects to a `Vec` under read guards and **drops them** before
  `register_newcomers` converts shapes across the rayon pool (touching no `World`) and only then takes the
  `PhysicsWorld` + `RapierHandles` write guards — pinned by `register_newcomers_parallel_section_holds_no_world_guard` (#4997).
- **Fix shape for every closed cycle: snapshot, then acquire** — never "hold both and be careful". A
  cycle here is almost never two systems disagreeing on an order; it is one site holding a guard *across a
  call* that acquires the pair the other way. `docs/engine/ecs.md` records the canonical direction; guards to
  confirm are live: `pull_dynamic_does_not_close_transform_global_transform_lock_cycle`
  (`crates/physics/src/sync.rs`, `Transform -> GlobalTransform`) and
  `get_actor_value_does_not_hold_actor_values_across_ruleset` (`crates/scripting/src/condition/tests.rs`,
  `CharacterRuleset -> ActorValues`), `unload_captures_do_not_invert_production_lock_orders` and
  `picked_up_restore_walks_meshes_before_taking_the_marker` (`byroredux/src/cell_loader/reference_state.rs`,
  #4982/#4983), `view_restamp_does_not_close_the_render_skip_lock_cycle` (`byroredux/src/player_body.rs`,
  #4991). The count of closed cycles is open and growing — the CI
  lock-order job (Dim 3) is what finds the next one; a second site acquiring `StringPool` mid-graph is the same class.
- **Helper order.** `set_linear_velocity` / `set_kinematic_translation` read `RapierHandles` via
  `world.query::<RapierHandles>()…copied()` (guard drops with the expression), *then* take
  `resource_mut::<PhysicsWorld>()`; callers (e.g. `character_controller_system`) must not already hold a
  `PhysicsWorld` guard. `PhysicsWorld` is a sink like `StringPool` (last, nothing taken under it — `docs/engine/ecs.md`):
  #5371 (f0c683ef1) was `forcegreet_system` / `eat_sleep_system` holding it across storage reads and the dialogue open,
  closing `Transform → PhysicsWorld → Transform` against `ragdoll_writeback_system`; pinned by
  `forcegreet_walk_with_real_physics_world` and `eat_walk_with_real_physics_world` under the detector. The same class
  elsewhere: `footstep_system` took `FootstepConfig` under the `AudioWorld` write guard (an `AudioWorld → FootstepConfig`
  edge on every footstep); the table is now cloned before the guard and `select_footstep_sound` is pure (#5556, ac710964e).
- **`ContactConfig`** is read via `try_resource` and snapshotted once per batch in `register_newcomers`, not re-locked per newcomer.
- **Cell-unload teardown (#1520).** `release_victim_rapier_bodies` (`unload.rs`) collects victims'
  `RapierHandles` under the read guard, drops it, then removes bodies from `PhysicsWorld`, before the despawn loop drops the handles.
- **Placement.** `physics_sync_system` is a Physics-stage parallel system after transform propagation and
  must not be co-scheduled with another system touching `PhysicsWorld` / `RapierHandles` / `Transform`
  (a declared conflict would already fail Dim 4's proof).
**Output**: `/tmp/audit/concurrency/dim_5.md`

### Dimension 6: Resource Lifecycle (GPU teardown ordering)
Paths: `crates/renderer/src/vulkan/context/{teardown,resize}.rs`, `crates/renderer/src/vulkan/{buffer,image,egui_pass,material}.rs`, `crates/renderer/src/vulkan/{acceleration,scene_buffer}/`
First step: `grep -n 'load-bearing' -B4 -A12 crates/renderer/src/vulkan/context/teardown.rs`
**Checklist**:
- **Destruction order is NOT reverse-creation.** After the `device_wait_idle` its contract requires,
  `destroy_allocator_owned_resources` (`context/teardown.rs`) imposes no cross-subsystem order. Only
  **three** orderings are load-bearing, each commented at its site: `skin_slots` before `skin_compute`
  (a real `VUID-vkFreeDescriptorSets-descriptorPool-parameter` otherwise); `frame_upscaler::destroy_allocations`
  after `destroy_device_objects`; `exposure` before the `Arc::try_unwrap`. The 1×1 placeholders are
  allocator-backed (destroy before `allocator.take()`) but not order-constrained (#4188). Flag a violation of one
  of those, an allocator-freed-early / `Arc::try_unwrap` hazard, or a resource that needs the allocator destroyed
  after it — not a "reverse-creation" mismatch.
- **Swapchain recreate.** G-buffer, SVGF, TAA, caustic, water-caustic, volumetrics, bloom, composite, ReSTIR
  reservoir buffers, the shadow filter and egui framebuffers are rebuilt; per-FIF history/accumulator images freed for
  every in-flight slot (`resize.rs`). The shadow filter's recreate submits nothing and its layout walk runs as a separate
  step, so a `may_be_in_flight` failure leaks the pass instead of destroying images the submit may still touch (#6063, 4a6128389).
  Since #6146 (4cd4bacbd) every pass's `initialize_layouts` goes through the shared `texture::submit_initial_layouts`, and the
  water-caustic / SVGF / caustic / TAA init arms plus the water-caustic and TAA resize arms release a failed pass through
  `release_after_failed_submit` (destroy only on `NotSubmitted`, leak otherwise); two source scans pin it
  (`every_initialize_layouts_body_uses_the_shared_submit`, `no_initialize_layouts_caller_destroys_without_the_in_flight_check`
  — since #6348, ad04a66b2, the second reads all six caller files incl. the pass-internal `taa.rs` / `svgf.rs` / `caustic.rs` /
  `shadow_filter.rs`, classifies every call site as guarded destroy, keep-alive arm that returns the error, or chained result,
  and pins per-file shape counts so a new caller fails instead of being skipped). A failed one-time layout walk fails
  closed (#6343, 9be0666fd): the G-buffer and bloom init/resize arms dispose through `release_after_failed_submit` and
  return `Err`, and the SVGF / TAA / caustic `recreate_on_resize` bodies return the error rather than `Ok(())` over
  UNDEFINED images (`a_failed_layout_walk_fails_closed_instead_of_warning_and_continuing`, `context/resize.rs`; source-shape
  only — no fault-injection hook exists, so the VUID before/after is unproven).
- **AS cleanup on shutdown.** All `BlasEntry` buffers, `TlasState` buffers and scratch released; per-entity skin outputs kept until the entity is destroyed.
- **Other GPU cleanup.** `scene_buffer`, `MaterialBuffer`, texture registry (incl. the `dynamic_rgba` staging
  arena), `EguiPass::destroy()` (`Option<EguiPass>` taken in `Drop`; its texture retirements are a
  frames-in-flight rider, #4988), a partially-built object's error arm (`GpuPerFrameTimers::new`, #4998), `TerrainPatchPass::destroy` (per-FIF readbacks, then pool / pipeline / layouts; the reserved patch meshes belong to the mesh registry) and `destroy_terrain_patch_blas` (every patch BLAS + the shared scratch, called from `AccelerationManager::destroy`),
  and `GpuImage` (`vulkan/image.rs`), which routes most passes' image
  create/bind/destroy through one lock rule: a poisoned allocator lock is recovered, not unwrapped (#4089).
- **Per-frame leaks.** Any descriptor / command-buffer / staging allocation created per frame but not freed or reset is HIGH.
**Output**: `/tmp/audit/concurrency/dim_6.md`

### Dimension 7: Worker Threads & Thread-Safety Bounds
Paths: `byroredux/src/streaming/`, `byroredux/src/texture_streamer.rs`, `byroredux/src/asset_provider/texture_prefetch.rs`, `byroredux/src/terrain_height.rs`, `byroredux/src/asset_provider/material/cdb.rs`, `byroredux/src/cell_loader/load_order.rs`, `byroredux/src/render/mod.rs`, `crates/bsa/src/read_at.rs`, `crates/video/src/lib.rs`, `crates/debug-server/src/{listener,system}.rs`, `crates/renderer/src/vulkan/allocator.rs`, `crates/ui/src/player.rs`, `crates/audio/src/`
First step: `grep -rnE 'thread::(spawn|Builder)|rayon::|mpsc::' --include='*.rs' crates byroredux tools | grep -v test` (a new worker thread outside this list is a coverage gap)
**Checklist**:
- **Streaming worker shutdown.** `WorldStreamingState::shutdown` takes the `worker` handle first, then
  sets the shared `cancel` flag and drops `request_tx` (#5491: dropping the sender alone does NOT stop the
  worker — `mpsc::recv` keeps returning every queued request until the queue is empty, and `payload_rx`
  outlives the join, so a backlog kept parsing; the worker checks `cancel` in `recv_next_live_request`),
  *then* `join_with_timeout` (poll on
  `is_finished`, no watcher thread); `Drop` only calls `shutdown(1 s)` as a safety net, so a prior explicit
  shutdown short-circuits it. The worker runs each cell under `catch_unwind`
  (`pre_parse_cell_panic_safe`), and each NIF's parse (`parse_one_nif`) and archive extraction (`extract_guarded`,
  #6119 3c57697c8) under its own, so one panicking file reads as a negative entry instead of discarding the whole
  cell's payload.
- **Worker ↔ main flow.** Parsed payloads move to the main thread over a channel; no shared `&mut World`.
  The worker parses on its own rayon pool (`build_stream_parse_pool`, sized so it does not starve the
  scheduler's global pool) and uses `Arc<TextureProvider>`; BSA/BA2/CSG extraction is lock-free positional
  reads (`crates/bsa/src/read_at.rs`, `ReadAt`, commit 1b8b21f3f — concurrent on Unix, kernel-serialised on
  Windows, #4999). A shared-cursor seek+read reintroduced anywhere returns interleaved wrong bytes (threaded tests per archive, #5000).
  The shared-archive registry (`open_shared`, `byroredux/src/asset_provider/archive.rs`) reuses a parsed reader only while its
  `ArchiveStamp` matches — length + mtime everywhere, plus `(dev, ino, ctime)` on Unix, since a same-length rewrite that
  restores the mtime otherwise served another entry's bytes as `Ok` (#5663, 04036a6fb); the path-keyed Starfield CDB caches
  in `asset_provider/material/cdb.rs` are documented process-lifetime and never re-stamped. Their index build also runs on a
  named `sf-cdb-index` prefetch thread (`prefetch_cdb_material_index`): the slot is installed before the thread is queued, so a
  lookup reaching a still-queued CDB parks on it, a shared build gate serialises prefetch and lazy builds (each holds a ~105 MB
  blob), a failed spawn falls back to the lazy build, and a panicking build is memoised, not retried per lookup (#5665, 988402d25).
  M39 texture promotions read archives on `TextureStreamer`'s own 2-thread rayon pool (`texture_streamer.rs`,
  `texture-stream-N`; `step` lands finished `FinishedRead`s over an `mpsc` channel and starts at most
  `MAX_READS_IN_FLIGHT`; the registry's `queue_promotion_bytes` runs on the main thread). It owns its own
  `TextureProvider`, so the shared archive registry's `ArchiveStamp` rule applies. Every pool job runs through `spawn_read`
  under `catch_unwind` (#6810, de596fafe; `a_panicking_read_is_reported_unreadable_not_fatal`): rayon aborts the process on an
  unhandled spawned-job panic, so a panicking read lands as an unreadable entry and still sends its `FinishedRead` (else `step`
  would keep the handle in `in_flight` forever) — a new pool spawn that bypasses `spawn_read` is the regression. A read that returns nothing or `Unusable` bytes backs the texture off for `READ_FAILURE_RETRY_FRAMES` (600) through `note_promotion_read_failed` instead of re-reading every frame (#7129, 590aded9e). The streamer's `TextureProvider` is rebuilt, on the main thread in `app_frame.rs`, when `asset_provider::archive_replacements()` has moved since it was built (`refresh_provider`, one atomic load per frame; #7144 99eee822c, the test drives explicit generations since #7164); in-flight reads land from the OLD file on the textures they were read for, so the registry stamps every queued upload with that counter (`set_source_generation_counter`) and only promotes a floor whose generation is the one the streamer reads from now (`set_promotable_generation`, #7169 522793dbb; `a_floor_promotes_only_under_the_generation_it_was_read_from`) — a promotion mixing mips from two files is the finding.
  Streamed NIFs' textures are prefetched on the stream pool (`prefetch_textures`, `PrefetchStore`): a panicking
  read must still complete its slot, and a withdrawn key must not be waited on. Terrain-layer height maps (#6195)
  generate on their own 2-thread rayon pool (`TerrainHeightService`, `terrain_height.rs`) over a cloned
  `texture_archive_snapshot()`; every job runs under `spawn_guarded` (`catch_unwind`), because rayon aborts the process on an
  unhandled spawned-job panic — a panicking job is reported `Failed` and the layer renders without height (#6233, 04e146b1d;
  `a_panicking_height_job_is_reported_failed`) — returning results through an `Arc<Mutex<Vec<…>>>` that only the main thread drains — a
  job must not touch the `World` or the registry, and the on-disk cache write is the shared state to check. External-material
  resolution (`merge_external_material`, which takes `&mut ImportedMaterial` + `&mut MaterialProvider` +
  `&mut StringPool`) is main-thread-only — moving it needs a real synchronisation story. The NIF import
  cache is read-only on the worker with write-back deferred to main.
- **Movie decode (`crates/video`).** Each `Movie` owns one `movie-decode` thread, spawned only by `spawn_reader` (pinned by
  `spawn_reader_is_the_only_reader_thread`; Bink 1 through `mercs2_bink`, Bink 2 through `byroredux-bink2`, whose slices fan out
  on up to `BINK2_THREADS` = 4 `std::thread::scope` threads) feeding a `sync_channel(FRAME_RING)`; `advance` drains it on the main
  thread, and `Drop` closes the ring (`rx = None`) before joining so a reader blocked in `send` wakes. No ECS guard or `World`
  reference crosses into the thread. The decode loop runs under `guard_decoder` (`catch_unwind`), so a decoder panic is sent as
  the frame failure and the movie ends `Failed` with a reason, not `Finished` (BIK-DEC-004, aa193bd9e, #6959/#6991; pinned by
  `a_decoder_panic_ends_the_movie_as_failed`). `Drop` still ignores the join result by design; a new reader path outside
  `spawn_reader` would lose the guard. `Movie::open` / `probe` run on the CALLER's thread (the frame loop) and go through `guard_open`, so a panic in the container parse, decoder setup or the whole-soundtrack decode ends the request as `VideoError::Decode` (#7128, 9e45e0261; `the_open_path_runs_under_the_open_guard`); the soundtrack WAV is a `TempPath` owned by `Audio` / `Movie`, removed on every exit including a panic (#7158, #7172 956aa1310; `the_soundtrack_wav_is_removed_on_every_way_out`).
- **Debug server (thread/lock shape only; command surface is `/audit-tooling`).** Per-client TCP threads
  never touch the `World`: they enqueue into a bounded queue (`MAX_QUEUED_COMMANDS`, client cap
  `MAX_CONCURRENT_CLIENTS`, `listener.rs`) and `DebugDrainSystem` (Late exclusive) executes on the main
  thread. Screenshot readback completes on a fence wait — check the drain/present race. The listener binds synchronously and
  retries an `AddrInUse` port every 100 ms for `BIND_RETRY_WINDOW` (5 s, b9749eb46) on the calling (main) thread, so startup
  can stall that long behind a dying previous engine; `--bench-hold` exits when the bind still fails (`main.rs`).
- **Allocator sharing.** `SharedAllocator = Arc<Mutex<Allocator>>` is cloned into the egui pass,
  volumetrics, SSAO, scene buffers, etc.; no holder keeps it locked across a queue submit or a
  fence wait. The egui pass takes the queue as a `Mutex` so its lock scopes to the `set_textures` submit (#1713).
- **`Send + Sync` bounds.** Component/Resource storage is reached only through World guards; no raw
  pointer crosses threads; Ruffle's `Player` (`crates/ui`, owned by `UiManager`) is not `Send` (non-`Send` video /
  audio backends), so it lives in the main loop outside the ECS and stays on one thread; kira runs its own audio thread behind `AudioWorld` — no ECS guard is held across a kira call.
- **Other rayon fan-outs.** `build_render_data` (`byroredux/src/render/mod.rs`) runs nested `rayon::join`
  branches that take ECS guards on pool threads — every branch must be read-only, or a write in one branch vs.
  a read in another is a cross-thread ABBA no CI lane drives. The plugin load-order walk
  (`cell_loader/load_order.rs`, `rayon::in_place_scope`, #3813) walks plugins in parallel; the `merge_from`
  fold must follow load order, never completion order. Its result is memoised in a process-wide
  `static SHARED_PARSE: SharedParseCache` (a `Mutex<Option<…>>` holding the session's `Arc<EsmIndex>` /
  `Arc<LoadOrder>`, #6123 588c87bc4); the lock must stay dropped across the parse itself (two racing callers both parse,
  last write wins — benign), and nothing acquires an ECS guard while holding it.
- Out of scope: parse-time `Material` translation is single-threaded (`/audit-nifal`).
**Output**: `/tmp/audit/concurrency/dim_7.md`

## Phase 3: Merge

1. Read all `/tmp/audit/concurrency/dim_*.md`. 2. Combine into `docs/audits/AUDIT_CONCURRENCY_<TODAY>.md`. 3. Remove cross-dimension duplicates.

Suggest: `/audit-publish docs/audits/AUDIT_CONCURRENCY_<TODAY>.md` (domain label *concurrency* for CPU-side
lock ordering / access declarations, *sync* for GPU-side semaphore/fence/barrier findings).
