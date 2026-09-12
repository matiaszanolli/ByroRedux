# Concurrency & Synchronization Audit — 2026-09-11

Repo: `/mnt/data/src/gamebyro-redux` · Branch `main` @ `b3db49fa` (post `Update SKILL
documentation across multiple audit commands`)

Scope: all 7 dimensions of `.claude/commands/audit-concurrency` — Vulkan queue/AS
sync, compute→AS→fragment chains, ECS lock ordering, scheduler access
declarations (regression guard), RwLock patterns (Resource↔Storage, physics),
GPU resource lifecycle/teardown, and worker threads/thread-safety bounds.
Depth: deep (per skill default). Dedup source: `gh issue list` snapshot at
`/tmp/audit/concurrency/issues.json` (56 OPEN issues at time of audit).

Each dimension was run as an independent agent that read the live source and
verified every named regression guard (the project's many previously-closed
concurrency cycles) against current code before reporting anything as NEW.
Per the skill's speculative-fix guardrail, Vulkan barrier/sync findings whose
only evidence is "this looks wrong" are marked **HYPOTHESIS** with a named
confirming signal, not shipped as a confirmed bug.

## Summary

| # | Dimension | New | Existing (re-confirmed) | Notes |
|---|-----------|-----|--------------------------|-------|
| 1 | Vulkan Queue & AS Sync | 3 (1 confirmed MEDIUM, 2 HYPOTHESIS) | 0 | #1782/#507945d8/#a476b256 guards intact |
| 2 | Compute → AS → Fragment Chains | 2 (1 MEDIUM, 1 LOW) | 0 | #3582/#1105/#931 guards intact |
| 3 | ECS Lock Ordering & Deadlock | 2 (1 MEDIUM, 1 LOW) | 0 | #2384/#3696/#2385/#2386/#3249 guards intact |
| 4 | Scheduler Access Declarations | 2 (2 LOW) | 1 (#3964) | #3111/#3652/#3580 guards intact |
| 5 | RwLock Patterns (Resource↔Storage, Physics) | 0 | 1 (#3964 — same as Dim 4, deduplicated) | #3303/#3441/#3580/#1520 guards intact |
| 6 | Resource Lifecycle (GPU teardown) | 2 (1 HIGH, 1 LOW) | 0 | #3658/#1483 guards intact |
| 7 | Worker Threads & Thread-Safety | 0 | 1 (#4089) | #1167 guard intact |

**Total findings reported: 13** (11 NEW + 2 Existing, after deduplicating
`#3964` which Dimensions 4 and 5 independently re-confirmed).

**Severity breakdown (NEW, confirmed only):**
- CRITICAL: 0
- HIGH: 1 (CONC-D6-01)
- MEDIUM: 3 (CONC-D1-03, CONC-D2-01, CONC-D3-01)
- LOW: 5 (CONC-D2-02, CONC-D3-02, CONC-D4-01, CONC-D4-02, CONC-D6-02)

**HYPOTHESIS (unconfirmed — needs validation-layer/RenderDoc confirmation, not
counted in the severity totals above):** 2 — CONC-D1-01 (would be HIGH),
CONC-D1-02 (would be MEDIUM).

**Existing (re-confirmed OPEN, not counted as new):** 2 — `#3964` (physics
`Ragdoll` access-declaration gap), `#4089` (GpuImage allocator poison-handling
asymmetry).

The most severe confirmed finding is **CONC-D6-01 (HIGH)**: `SceneBuffers`'
terrain-tile staging pool holds a non-optional `SharedAllocator` clone that
nothing ever releases, so `Arc::try_unwrap` fails on every engine shutdown,
triggering the leak-guard branch that deliberately abandons the `VkDevice`/
`VkSurfaceKHR`/`VkInstance` rather than doing a clean teardown.

---

# Dimension 1 — Vulkan Queue & Acceleration-Structure Sync

Scope read: `context/draw.rs`, `context/sync_and_acquire_frame.rs`,
`context/dispatch_skin_and_cluster.rs`, `context/skinned_blas_refit.rs`,
`context/resize.rs`, `context/resources.rs`, `context/teardown.rs`,
`vulkan/sync.rs`, `vulkan/texture.rs`,
`vulkan/acceleration/{mod,blas_static,blas_skinned,tlas,memory,predicates}.rs`,
`vulkan/scene_buffer/upload.rs`, `byroredux/src/app_frame.rs`.

Checklist items verified **clean** (regression guards still intact): queue
Mutex held across submit/present at both sites; frame-in-flight discipline;
acquire→submit→present semaphore chain; #1782 deferred scratch retirement
(both grow/shrink sites route through `pending_destroy_scratch`, the
`blas_skinned.rs` immediate free is the documented deliberate exception);
#507945d8 AS-build-input access flag (`SHADER_READ` at
`ACCELERATION_STRUCTURE_BUILD_KHR`, not `ACCELERATION_STRUCTURE_READ_KHR`, in
both `tlas.rs` and the skin chain); #a476b256 deferred AS destruction (every
eviction/drop routes through `pending_destroy_blas`; shutdown drains behind
`device_wait_idle`); swapchain recreate is `device_wait_idle`-first.

### CONC-D1-01: static `build_blas_batched` emits no scratch-serialize barrier before its first build, while a previously-submitted frame's skinned refits may still be executing against the same shared scratch
- **Severity**: HYPOTHESIS — would be HIGH if confirmed
- **Dimension**: Vulkan Queue & AS Sync
- **Location**: `crates/renderer/src/vulkan/acceleration/blas_static.rs:694-727` (barrier gated on `if i > 0`), vs. `crates/renderer/src/vulkan/acceleration/blas_skinned.rs:294-313` (the sibling, which self-emits before `i == 0`, #1300)
- **Status**: NEW
- **Description**: `AccelerationManager::blas_scratch_buffer` is a single shared allocation written by three call sites: static batched builds, skinned first-sight builds, and skinned refits. The skinned path emits a scratch-serialize barrier *before its first build* precisely because a host fence-wait between submissions does not establish a device-side memory dependency (#1300). The static path's loop only emits the barrier for `i > 0`, so its first `cmd_build_acceleration_structures` in the one-time submission has none. The static path is worse off than the case #1300 covers: `build_blas_batched` runs from `step_streaming`/`restore_missing_static_blas_for_draws` (`byroredux/src/app_frame.rs:263`), both *before* `draw_frame`'s top-of-frame `wait_for_fences`, so a prior frame's skinned BUILD/refit against the same scratch address may still be executing on the GPU when this batch submits. The hazard exists only when `scratch_needs_growth` is false (steady state; a growth event replaces the buffer safely via `pending_destroy_scratch`).
- **Evidence**:
  ```rust
  // blas_static.rs:694-698
  let build_result = submit_one_time(device, queue, command_pool, transfer_fence, |cmd| {
      for (i, p) in prepared.iter().enumerate() {
          if i > 0 {
              self.record_scratch_serialize_barrier(device, cmd);
  ```
  vs.
  ```rust
  // blas_skinned.rs:307-313
  if !prepared.is_empty() {
      self.record_scratch_serialize_barrier(device, cmd);
  }
  for (i, p) in prepared.iter().enumerate() {
      if i > 0 { self.record_scratch_serialize_barrier(device, cmd); }
  ```
  The project's own codified rule agrees a barrier is required here:
  `acceleration/predicates.rs:646-681` (`ScratchUser::CrossSubmissionBuildWithFenceWait` ⇒ `true`)
  and its test `acceleration/tests/scratch_tests.rs:400-422`, whose doc comment
  enumerates exactly two sites relying on the rule — `refit_skinned_blas` and
  `build_skinned_blas_batched_on_cmd`'s `i == 0`. The static path's `i == 0` is
  absent from that enumeration.
- **Impact**: Write-after-write on the shared AS-build scratch region between an in-flight skinned BUILD/refit and a streaming static BLAS build. Undefined BVH contents for the loser: garbled/exploded RT shadows/reflections/GI on the streamed statics or the skinned actor, persisting until the next refit-count rebuild threshold; worst case `VK_ERROR_DEVICE_LOST`. RT-only blast radius; raster unaffected.
- **Trigger Conditions**: `blas_scratch_buffer` already large enough (no growth) AND a prior frame's skinned BUILD/refit command buffer still executing when `submit_one_time` lands the static batch AND the driver actually overlaps the two. Realistic on: walking into a new cell with NPCs on screen, or `restore_missing_static_blas_for_draws` firing under BLAS-budget pressure with skinned actors present.
- **Verification Path**: Validation-layer only — `BYRO_VALIDATION=1` with synchronization validation, on a cell-load-while-NPCs-visible run. Concrete signal: `SYNC-HAZARD-WRITE-AFTER-WRITE`/`WRITE-AFTER-READ` naming the shared scratch `VkBuffer` with a cross-submission attribution at `vkCmdBuildAccelerationStructuresKHR`. Older per-submission syncval will NOT flag this (the project's own caveat, `predicates.rs:652-655`) — a clean run on an old SDK is not a disproof. Secondary signal: RenderDoc showing the static batch's first build and the prior frame's refit both resolving to the same `scratchData.deviceAddress`. Not visible to `cargo test`.
- **Related**: #1300, #983, #1140, #642; CONC-D1-02 (same rule, different resource).
- **Suggested Fix**: Mirror #1300 — hoist an unconditional `self.record_scratch_serialize_barrier(device, cmd);` before the loop in `blas_static.rs::build_blas_batched`, and extend `scratch_tests.rs`'s doc enumeration to name this third site. Ship only after validation-layer confirmation.

### CONC-D1-02: on frames with no skinned actors, nothing publishes prior-submission static BLAS writes to the TLAS build's reads
- **Severity**: HYPOTHESIS — would be MEDIUM if confirmed
- **Dimension**: Vulkan Queue & AS Sync
- **Location**: `crates/renderer/src/vulkan/context/skinned_blas_refit.rs:827-836` (the only pre-TLAS `AS_WRITE → AS_READ` barrier, nested inside the skin-path guards), `crates/renderer/src/vulkan/context/dispatch_skin_and_cluster.rs:222-318` (the only other AS barrier, which runs *after* `build_tlas`)
- **Status**: NEW
- **Description**: `build_tlas` reads every referenced BLAS. The only barrier in the frame publishing AS writes to that read is `skinned_blas_refit.rs:829-836`, emitted only inside the skinned path (requires `skin_compute`, `accel_manager`, a bone buffer, a non-empty dispatch list). A static-only frame (actor-free interior, headless bench, or an early-return on `bind_inverse_upload_failed`) records **no** AS barrier before `build_tlas`, even though the referenced static BLAS were written by a different submission (`step_streaming`'s `build_blas_batched`, or `restore_missing_static_blas_for_draws` earlier in the *same* frame). By the project's own cross-submission rule, that write→read needs a device-side dependency that isn't present.
- **Evidence**: grepping `ACCELERATION_STRUCTURE` across `context/` returns AS barriers at exactly two places — the conditional pre-TLAS one in `skinned_blas_refit.rs:829`, and the unconditional one in `dispatch_skin_and_cluster.rs:310`, which runs *after* `build_tlas` (publishing the TLAS write to ray-query consumers, not the BLAS writes to the TLAS build).
- **Impact**: If the strict Vulkan reading holds, a TLAS built on a static-only frame can traverse BLAS whose build writes aren't yet visible — missing/corrupt RT shadows/reflections/GI for newly-streamed meshes on that frame, self-healing the next frame with a skinned actor (or never, in an actor-free cell). If the permissive reading holds (fence-signal domain chaining through the next submit), this is a no-op — which is exactly why it's filed as HYPOTHESIS.
- **Trigger Conditions**: A frame with rigid TLAS-eligible meshes whose BLAS were built in a prior/one-time submission, and no skinned dispatch that frame. Most observable right after a cell load or BLAS-eviction restore in an actor-free interior.
- **Verification Path**: Validation-layer (cross-submission syncval) or RenderDoc only. Concrete signal: `SYNC-HAZARD-READ-AFTER-WRITE` at `vkCmdBuildAccelerationStructuresKHR` (TOP_LEVEL) naming a BLAS buffer written by a previous submission, on an actor-free cell load. Visible-artifact signal: newly streamed statics missing from RT shadows/reflections only on actor-free frames, popping in once an NPC appears. Not reachable from `cargo test`.
- **Related**: CONC-D1-01 (same rule, scratch buffer); #2931.
- **Suggested Fix**: If syncval confirms, emit the `ACCELERATION_STRUCTURE_BUILD_KHR`/AS_WRITE → AS_READ barrier unconditionally in `dispatch_skin_and_cluster.rs` immediately before `accel.build_tlas(...)`, rather than leaving it nested in the skinned path's control flow.

### CONC-D1-03: two blocking one-time submits (build + compaction, with a `WAIT` query readback between them) run inside the per-frame path via `restore_missing_static_blas_for_draws`
- **Severity**: MEDIUM
- **Dimension**: Vulkan Queue & AS Sync
- **Location**: `byroredux/src/app_frame.rs:263` → `crates/renderer/src/vulkan/context/resources.rs:374-503` → `crates/renderer/src/vulkan/acceleration/blas_static.rs:694`, `:814-823`, `:954`
- **Status**: NEW
- **Description**: `restore_missing_static_blas_for_draws` is called every frame from the render driver; when a TLAS-eligible rigid handle lacks a BLAS it runs the full `build_blas_batched` pipeline: a `submit_one_time` (builds + compaction-size queries) with a host fence-wait, then `get_query_pool_results` with `vk::QueryResultFlags::WAIT` (a second host stall), then a second `submit_one_time` for the compaction copies with another fence-wait — up to `MAX_STATIC_BLAS_RESTORES_PER_FRAME = 256` meshes per frame, repeated every frame until the visible set is restored. Synchronization-correct, but load-time-shaped work executing in the frame loop; the fence wait lands on the same graphics queue as the still-in-flight previous frame.
- **Evidence**: `app_frame.rs:263` calls this per-frame, before `draw_frame`; `resources.rs:485-491` invokes `build_blas_batched`; `blas_static.rs` does two `submit_one_time` calls with a `WAIT` query readback between them; `constants.rs:166` sets the 256-mesh cap.
- **Impact**: Multi-millisecond-to-second CPU+GPU stalls whenever LRU eviction has removed BLAS still visible — sustained hitching on over-budget cells, not a one-off load cost. Also the mechanism that makes CONC-D1-01's cross-submission window realistic on ordinary frames.
- **Trigger Conditions**: `static_blas_bytes` near `blas_budget_bytes` with a large visible rigid set, so eviction keeps reclaiming BLAS the next frame's draw list needs (the oscillating case #3540 bounds but doesn't eliminate). Large exteriors/Starfield city cells on cards near the 6 GB RT minimum.
- **Verification Path**: `cargo test` cannot see it. Reproduce with `cargo run --release -- … --bench-frames 300 --bench-hold` on a large exterior; correlate frame-time spikes with the `"Restored {count} missing static shadow BLAS before TLAS build"` debug log, or `RtIntegrityStats` BLAS counters via `byro-dbg`. Concrete signal: a spike in the pre-`draw_frame` segment (outside `fence_wait_ns`/`cmd_record_ns`/`submit_present_ns`) on frames where that log fires.
- **Related**: #3540 (the per-frame cap), #1449 (why eviction stays deferred), CONC-D1-01.
- **Suggested Fix**: Record the restore builds into the frame command buffer (as #911 did for skinned first-sight BUILDs), or move the restore into the streaming step in `about_to_wait` with an explicit per-frame budget, so no host fence-wait sits in the render driver.

---

# Dimension 2 — Compute → AS → Fragment Chains

Scope: skin palette/vertex compute → skinned BLAS refit → TLAS → fragment/compute
ray-query consumers; cross-frame ping-pong (SVGF/TAA/caustic/water-caustic/
volumetrics/ReSTIR); the volumetrics `tlas_written` latch (#1105); the bloom
per-mip RAW chain (#931); the caustic CLEAR→COMPUTE→FRAGMENT chain; the
MaterialBuffer host-upload placement (R1).

### CONC-D2-01: the ground-cover counter readback copies a buffer the scatter dispatch just wrote, with no COMPUTE → TRANSFER dependency
- **Severity**: MEDIUM
- **Dimension**: Compute → AS → Fragment Chains
- **Location**: `crates/renderer/src/vulkan/groundcover.rs:1231-1249` (barrier at 1232-1239, copy at 1244-1249); helper at `groundcover.rs:1485-1511`
- **Status**: NEW
- **Description**: `GroundCoverPipeline::record_scatter` dispatches `groundcover_scatter.comp`, which writes `counter_buffer` via atomics (per-chunk cursors, §11.3 histogram, `atomicMin`/`atomicMax` extrema), then emits exactly one publish barrier and immediately `vkCmdCopyBuffer`s from that same buffer into the per-FIF `counter_readback[frame]`. The barrier's dst scope is `DRAW_INDIRECT | VERTEX_SHADER` / `INDIRECT_COMMAND_READ | SHADER_READ` — it names neither `TRANSFER` stage nor `TRANSFER_READ` access, so the copy's read has no dependency on the compute write.
- **Evidence**:
  ```rust
  buffer_barrier(
      device, cmd,
      vk::PipelineStageFlags::COMPUTE_SHADER, vk::AccessFlags::SHADER_WRITE,
      vk::PipelineStageFlags::DRAW_INDIRECT | vk::PipelineStageFlags::VERTEX_SHADER,
      vk::AccessFlags::INDIRECT_COMMAND_READ | vk::AccessFlags::SHADER_READ,
  );
  device.cmd_copy_buffer(cmd, counters.buffer, self.counter_readback[frame].buffer, &[..]);
  ```
  The two sibling edges in the same file are correct by contrast (`groundcover.rs:1186-1193`, `:1306-1314`), making the readback edge the sole outlier. This is a single-command-buffer hazard, not covered by the all-slots `wait_for_fences` at `context/sync_and_acquire_frame.rs:63-67` (which only structurally covers cross-frame cases).
- **Impact**: `GroundCoverPipeline::harvest` decodes the readback into `GroundCoverStats` (blade counts, density histogram, extrema) — EXAL §11.3 ground-cover density tuning reads these numbers directly. Rendering itself is unaffected (the draw uses the correctly-published edge); the failure mode is silently wrong telemetry driving tuning decisions, plus sync-validation noise.
- **Trigger Conditions**: Any exterior frame with ground cover active (`frame_chunk_count > 0`). Whether it manifests is driver-dependent — a driver that doesn't overlap the copy with the still-running dispatch produces correct numbers anyway.
- **Verification Path**: `BYRO_VALIDATION=1` with synchronization validation on an exterior cell with ground cover. Expected signal: `SYNC-HAZARD-READ-AFTER-WRITE` naming `vkCmdCopyBuffer` as reader and the scatter dispatch as prior writer of `counter_buffer`. Absent that signal, treat as spec-pedantic rather than observed.
- **Related**: #4054 (EXAL ground cover), #4056 (EXAL ground cover Phase 3, OPEN).
- **Suggested Fix**: Widen the trailing barrier's dst scope to add `TRANSFER`/`TRANSFER_READ`, or emit a second `COMPUTE_SHADER/SHADER_WRITE → TRANSFER/TRANSFER_READ` edge before the copy. Purely additive — same class of change #2403 made for the skinned-vertex publish mask. Do not ship unvalidated.

### CONC-D2-02: five HOST→stage barriers justify themselves with a false premise ("required by spec even for HOST_COHERENT memory")
- **Severity**: LOW
- **Dimension**: Compute → AS → Fragment Chains
- **Location**: `crates/renderer/src/vulkan/context/build_and_upload_instances.rs:984-1004`; `crates/renderer/src/vulkan/context/dispatch_skin_and_cluster.rs` (cluster-cull HOST barrier); `crates/renderer/src/vulkan/volumetrics.rs:1170-1181`
- **Status**: NEW — documentation/rationale defect, not a sync defect
- **Description**: Three sites emit `HOST/HOST_WRITE → {VERTEX,FRAGMENT,COMPUTE,DRAW_INDIRECT}` barriers whose comments assert the barrier is required by the Vulkan spec even for `HOST_COHERENT` memory. That premise is backwards: Vulkan's host-write ordering guarantee (§7.9) puts host writes flushed before `vkQueueSubmit` into the first access scope of an implicit dependency covering the whole submitted batch. Every write here is a mapped write performed during recording, strictly before submit, so it's already visible without the barrier.
- **Evidence**: all buffers involved (`SceneBuffers` SSBOs/UBOs, SVGF/TAA/bloom/composite param UBOs, `VolumetricsParams`) are written via `write_mapped`/`write_mapped_prefix` on host-visible memory earlier in the same `draw_frame` call, before `queue_submit`; no path writes them from a separate host thread concurrently with an in-flight batch.
- **Impact**: None at runtime — harmless over-synchronization. The cost is that these load-bearing-for-audits comments state an incorrect rule; a future audit reasoning from the stated premise could add unwarranted HOST barriers elsewhere, or (less likely) conclude a genuinely missing edge is present when it isn't.
- **Trigger Conditions**: N/A — a rationale defect, reachable only by a reader.
- **Verification Path**: Vulkan 1.3 spec §7.9 "Host Write Ordering Guarantees". No GPU run needed. Note: *removing* the barriers is a real behavior change (falls under the speculative-fix policy); correcting the comment does not.
- **Related**: #909, #961, #1397 (the folds that produced these comments).
- **Suggested Fix**: Comment-only — reword to the accurate rationale (defense against a future non-coherent memory type or post-recording host write) rather than asserting the spec requires it. Do not remove the barriers.

## Verified intact (Dimension 2, no finding)
- **#3582** second-consumer publish barrier: `record_skinned_blas_refit`'s three-bit dst mask (`ACCELERATION_STRUCTURE_BUILD_KHR | FRAGMENT_SHADER | COMPUTE_SHADER`) confirmed complete against an independent grep of every `.comp`/`.frag` consumer of `SkinnedVertexRef`.
- Full M29 skin→BLAS→TLAS→fragment chain confirmed unbroken end to end.
- **#1105** volumetrics `tlas_written` latch set/assert/reset symmetry intact; single call site, no leak path.
- **#931** bloom per-mip RAW chain intact on both pyramids including `up_mips[0]`.
- Caustic CLEAR→COMPUTE→FRAGMENT intact on both the parked-camera and moving-camera arms.
- All five cross-frame ping-pong indexings (SVGF, TAA, volumetrics, ReSTIR, water-caustic) read the correct previous slot.
- R1 MaterialBuffer upload placement unchanged (still pre-recording, host-mapped).

---

# Dimension 3 — ECS Lock Ordering & Deadlock

Scope read in full: `crates/core/src/ecs/{world,query,resource,lock_tracker}.rs`,
`docs/engine/ecs.md` §Lock-ordering policy, every multi-lock system body under
`byroredux/src/systems/`, plus the cross-crate helpers those systems call while
holding guards.

Checklist items 1 (TypeId-sorted acquisition), 3 (#2384/#3696 check-before-insert),
4 (#2385 `GRAPH` poison recovery), 5 (#2386/#3249 recursive-read warning, not
panic), 6 (guard lifetime, with one exception below), and 7 (poison handling via
`storage_lock_poisoned`/`resource_lock_poisoned`) all verified **intact** with
line-level citations. Item 2 (AccessReport) confirmed present, deferred to
Dimension 4.

### CONC-D3-01: `submersion_system` samples `TotalTime`/`WindField` underneath the `WaterPlane`/`WaterVolume` storage guards — the only one of four wave-sampling sites that inverts the documented snapshot-before-storage discipline
- **Severity**: MEDIUM
- **Dimension**: ECS Lock Ordering & Deadlock
- **Location**: `byroredux/src/systems/water.rs:154-188` (guards dropped at 245-246); second resource acquired at `crates/physics/src/water.rs:390-394`
- **Status**: NEW
- **Description**: `submersion_system` binds `world.query::<WaterPlane>()` (154) and `world.query::<WaterVolume>()` (164), then only at 181-188 acquires `try_resource::<TotalTime>()` and, inside the closure with that guard still live, calls `weather_wave_adjustment(world, time.0)`, which itself acquires `try_resource::<WindField>()`. Both water storage guards stay live until an explicit `drop` at 245-246. This records four lock-order edges nothing else in the tree records: `WaterPlane/WaterVolume → TotalTime/WindField`.
- **Evidence**: The other three consumers of the identical wave-parameter pair all snapshot the frame-global resources into plain scalars **before** taking any water storage guard — one of them (`character.rs:1043-1056`) documents this explicitly as "resource-snapshot-before-storage discipline (#3265)". `crates/physics/src/water.rs:637-645` and `byroredux/src/render/water.rs:102-131` both follow the same order. Only `water.rs:181-188` inverts it. No reverse edge exists anywhere in the workspace today (checked every `WindField`/`WaterPlane`/`WaterVolume` acquisition site), so there is no live cycle — this is latent, not live.
- **Impact**: (a) The moment any code takes `TotalTime`/`WindField` before a water storage read — the natural spelling, since 3 of 4 existing sites do resource-first — the graph closes a real ABBA cycle and a `BYRO_LOCK_ORDER_CHECK=1` run aborts. (b) `submersion_system` is currently `add_exclusive_with_access` (`byroredux/src/boot/schedule/late.rs:144-155`), so today's safety is circumstantial — promoting it to a parallel lane is a one-line change with no compile-time or test-time signal.
- **Trigger Conditions**: A second site acquiring `TotalTime`/`WindField` before `WaterPlane`/`WaterVolume`, plus either `BYRO_LOCK_ORDER_CHECK=1` or `submersion_system` moving out of the exclusive lane.
- **Related**: #3265 (the discipline violated), #2388/#313 (inverted-pair debug aborts), `docs/engine/ecs.md` §Canonical acquisition order.
- **Suggested Fix**: Hoist the frame-global sample above the storage guards to match `player_water_state` exactly — move the `wave_adjustment` binding before line 154's `query::<WaterPlane>()`, as a plain `Option` scalar. Three lines moved, no behavioral change.

### CONC-D3-02: `SubtreeCache`'s position in the animation lock order lives only in `systems/animation.rs` comments — the exact gap #3651 exists to close
- **Severity**: LOW
- **Dimension**: ECS Lock Ordering & Deadlock
- **Location**: `byroredux/src/systems/animation.rs:54-68`, `687-803`, `841-1017`; `docs/engine/ecs.md` (canonical-order cluster list, which stops at `AnimationPlayer`)
- **Status**: NEW
- **Description**: `animation_system_inner` holds a strictly longer chain than documented: `AnimationClipRegistry → NameIndex → SubtreeCache → {AnimationPlayer | AnimationStack | Transform | AnimationTextKeyEvents | RootMotionDelta | Animated*}`, with `SubtreeCache`'s miss path additionally recording `NameIndex → Children`/`Name`. `docs/engine/ecs.md` never names `SubtreeCache`. Currently there is exactly one consumer (`systems/animation.rs`) and no contradictory direction exists anywhere, so nothing is broken today.
- **Evidence**: `grep` for `SubtreeCache` resource access returns hits only in `systems/animation.rs`; the internals are individually correct (miss path drops its read before the write, no same-type panic risk; `Children`-before-`Name` matches the canonical tail).
- **Impact**: Documentation-completeness only. The risk is a future second consumer (e.g. NPC/facial-animation or a debug-inspection path) re-deriving the direction from scratch and picking `SubtreeCache` under `Transform`/`AnimationStack`, closing a cycle against this system's every-frame edges — exactly the class #3651 was filed to close for three other clusters.
- **Trigger Conditions**: Any new `SubtreeCache` consumer authored without reading `animation.rs`'s inline comments.
- **Related**: #3651, #2400, #824/#827 (NameIndex-before-Name), #2924.
- **Suggested Fix**: Extend the animation cluster line in `docs/engine/ecs.md` to `AnimationClipRegistry → NameIndex → SubtreeCache → AnimationPlayer/AnimationStack → Transform`, noting the `Children → Name` miss-path tail. Pure doc change.

---

# Dimension 4 — Scheduler Access Declarations (regression guard)

M27 (parallel dispatch) + R7 (access declarations) are closed; this pass
verifies they stayed closed. All five checklist items re-derived from current
source: the three-variant `AccessConflict` shape with pessimistic `Unknown`
fallback; the four migration KPIs enforced at boot (`install_runtime_registries`)
plus two `cargo test`-reachable copies with non-vacuity floors; the exclusive
phase (`audio_system`/`spin_system` still exclusive, `player_controller_system`
still the single merged Early-parallel registration); `Scheduler` still not a
`Resource`; and a full single-writer/multi-reader sweep of every declared
resource across all five stage files, confirming `WindField` (#3111) is the
only intentional writer-after-reader-batch inversion and that #3652's
billboard/footstep-vs-camera_follow fix is still in place.

### CONC-D4-01: `footstep_system`'s #3652 stage placement is the only half of that fix with no regression pin
- **Severity**: LOW
- **Dimension**: Scheduler Access Declarations
- **Location**: `byroredux/src/boot/schedule/late.rs:82-107`; guards at `byroredux/src/scheduler_access_tests.rs:597-661`
- **Status**: NEW
- **Description**: #3652 moved both `make_billboard_system` and `footstep_system` out of `Stage::PostUpdate` into the `Stage::Late` exclusive lane because both read the camera's `GlobalTransform`, authored by `camera_follow_system` in the Late parallel batch. `make_billboard_system` got a dedicated pin (`billboard_runs_after_camera_follow_in_late`) asserting both its stage/order and that it's absent from every other stage. `footstep_system` got neither. Since `analyze_pair` is intra-stage only, a regression that moves `footstep_system` back to `PostUpdate` (or into Late's parallel batch) is invisible to every existing counter and test.
- **Evidence**: `late.rs:98-107` registers `footstep_system` via `add_exclusive_with_access` in `Stage::Late`; grepping the crate for a stage assertion on it returns nothing beyond unrelated resource-catalog/distance-accumulation tests.
- **Impact**: No live defect — placement is currently correct. The exposure is that the *reason* (a 16-line comment) is enforced by prose only; a future stage reshuffle could reintroduce the one-frame-stale spatial-audio trigger position #3652 fixed, with a fully green test suite.
- **Trigger Conditions**: Any edit that moves `footstep_system` between stages or into a parallel batch.
- **Related**: #3652, #3180 (the `submersion_system` precedent, which *is* pinned), #848.
- **Suggested Fix**: Extend `billboard_runs_after_camera_follow_in_late` (or add a sibling) to assert the same three properties for `footstep_system`.

### CONC-D4-02: WindField's only same-stage reader runs in Early's parallel phase — the accepted one-frame lag is neither stated nor pinned
- **Severity**: LOW
- **Dimension**: Scheduler Access Declarations
- **Location**: `byroredux/src/boot/schedule/early.rs:19-83`; consumer chain `byroredux/src/systems/character.rs:1043-1052`
- **Status**: NEW
- **Description**: #3111 kept `weather_system` in `Stage::Early` but moved it to the exclusive phase (which runs after the stage's parallel batch). `player_controller_system` — the reader the fix targeted — is in that parallel batch, so it reads the *previous* frame's `WindField` value. Correct and deliberate, but the registration comment says only "the controller sees one stable snapshot," never which frame's. The directly analogous accepted cross-stage lag (#3653, `ParticleEmitter::rate`) is both spelled out in a 20-line comment and pinned by a dedicated test; this one is neither.
- **Evidence**: `Scheduler::run` runs `data.parallel` then `data.exclusive` per stage (`scheduler.rs:497-515`); `player_controller_system` is `add_to_with_access` (parallel), `weather_system` is `add_exclusive_with_access`, both `Stage::Early`. `analyze_pair` never compares a parallel entry against an exclusive one, so no counter can see this ordering.
- **Impact**: Practically nil in magnitude (wave-scroll offset / wind-wave scale change over seconds-to-minutes timescales). The structural exposure: any new Early-parallel WindField consumer silently inherits the same lag, and a maintainer reading the comment would reasonably believe the controller sees this frame's wind.
- **Trigger Conditions**: Adding a lag-sensitive WindField consumer to the Early parallel batch, or "fixing" the lag by flipping `weather_system` back to `add_to_with_access` (which would at least fail loudly via `known_conflict_count`).
- **Related**: #3111, #3653 (the documented-and-pinned precedent), #3123.
- **Suggested Fix**: One sentence in the `early.rs:64-67` comment stating the Early parallel batch reads the previous frame's WindField and why that's acceptable; optionally extend the existing pin to assert the phase relationship on the built schedule.

### CONC-D4-03 / CONC-D5-01: `physics_sync_system` acquires the `Ragdoll` storage via `crates/physics/src/water.rs` without declaring it
- **Severity**: MEDIUM
- **Dimension**: Scheduler Access Declarations / RwLock Patterns (reported independently by both Dimension 4 and Dimension 5 — merged here as one finding)
- **Location**: `crates/physics/src/water.rs:487` and `:747`; declaration at `byroredux/src/boot/schedule/physics.rs:7-49`
- **Status**: **Existing: #3964** (OPEN) — re-confirmed still present in current code; not a new finding
- **Description**: The buoyancy pass reached from `physics_sync_system` (Phase 2.5) takes `world.query::<Ragdoll>()` twice; the system's declared `Access` lists 12 component reads and 2 writes, none of them `byroredux_physics::Ragdoll`. `every_parallel_system_declares_everything_it_acquires` only scans `crates/physics/src/sync.rs` for this system and deliberately doesn't follow the cross-file hop into `water.rs`, so this omission is outside that guard's scan. Every other type acquired in `water.rs` *is* declared — this is a single omission, not systemic.
- **Impact**: No live hazard today: `Stage::Physics` holds exactly one parallel system, so the analyzer has no pair to misclassify. It narrows the boot deadlock proof and would matter the moment a second Physics-stage parallel system writing `Ragdoll` is added.
- **Trigger Conditions**: A second parallel registration in `Stage::Physics`.
- **Related**: #3964 (OPEN), #3492 (added the read), #4064, #3951.
- **Suggested Fix**: Add `.reads::<byroredux_physics::Ragdoll>()` to the declaration and register the cross-file hop `(PHYSICS_WATER_SRC, "apply_buoyancy_with_scratch")` in `PARALLEL_SYSTEMS`, per #3964's own documented mechanism.

---

# Dimension 5 — RwLock Patterns: Resource ↔ Storage & Physics Step

Arbiter doc: `docs/engine/ecs.md` §"Lock-ordering policy" (read in full).
**No new violations confirmed.** Every closed-cycle fix named in the checklist
was verified present by reading the actual function bodies:

- Phase 1 `collect_newcomers` collects under read guards and drops them before `register_newcomers` takes `PhysicsWorld`/`RapierHandles` write guards (`crates/physics/src/sync.rs:128-133`, 846-893, 905-1060).
- `pull_dynamic`'s #3303 two-pass split still holds: `Parent`+`GlobalTransform` resolved and dropped in one block, `Transform` read/write in a separate later block, so the `GlobalTransform → Transform` edge that closed the cycle against `make_transform_propagation_system` cannot be recorded (`crates/physics/src/sync.rs:1138-1274`; pin test `pull_dynamic_does_not_close_transform_global_transform_lock_cycle` intact).
- `evaluate_function`'s `GetActorValue` arm (#3441): `ActorValues` borrow confined to a scoped block, `CharacterRuleset` acquired only after (`crates/scripting/src/condition.rs:457-530`).
- `combat_approach_line_of_sight_reaches`, `validate_equipment`, `condition::evaluate`'s `GetEquipped` arm, `IsSceneActionComplete`, and `PapyrusPlayerEntity → QuestStageAdvancedBatch` (#3580/#3819 fixes) all confirmed intact via direct read.
- `PhysicsWorld`-as-sink property swept across all 55 guard sites in `byroredux/src`, `crates/physics/src`, `crates/scripting/src`, `crates/save/src` — no site takes a storage guard underneath the resource guard.
- `set_linear_velocity`/`set_kinematic_translation` drop their `RapierHandles` read before taking `resource_mut::<PhysicsWorld>()`; every production caller verified to hold no `PhysicsWorld` guard at the call site.
- `ContactConfig` snapshotted once per batch, not re-locked in the per-newcomer loop.
- Cell-unload teardown (#1520): `release_victim_rapier_bodies` collects into scratch `Vec`s under read guards before taking `PhysicsWorld` write, and runs before `despawn_batch`.
- Single-threaded placement: `physics_sync_system` is the only `Stage::Physics` registration in the tree.

The one deviation found (`physics_sync_system`'s undeclared `Ragdoll` read) is
the same issue Dimension 4 found independently — see **CONC-D4-03/CONC-D5-01**
above; not double-counted.

Dimension 5: 0 new findings.

---

# Dimension 6 — Resource Lifecycle (GPU teardown ordering)

Scope: `context/teardown.rs`, `context/resize.rs`, `buffer.rs`, `acceleration/`,
`egui_pass.rs`, `scene_buffer/`, `image.rs`, `material.rs`, plus the
svgf/gbuffer/composite/ssao/taa/caustic/water_caustic/volumetrics/bloom
destroy+resize paths.

Checklist verified **held**: the four load-bearing local orderings (skin_slots
before skin_compute; frame_upscaler after destroy_device_objects; exposure
before `Arc::try_unwrap`; the corrected #3658 understanding that there is no
required cross-subsystem reverse-creation-order); `device_wait_idle` runs
before `destroy_allocator_owned_resources`; the #1483 hoist has no
allocator-dependent destroy outside its guard; swapchain recreate rebuilds
every extent-dependent subsystem with old per-FIF resources freed first; AS
cleanup drains every pending-destroy list and slot on shutdown; egui/registry/
SSBO cleanup all explicit; no per-frame descriptor/command-buffer/image/buffer
allocation found anywhere in the renderer's per-frame path.

### CONC-D6-01: `SceneBuffers::terrain_tile_staging_pool` holds a `SharedAllocator` clone that nothing can release, so `Arc::try_unwrap` fails on every shutdown
- **Severity**: HIGH
- **Dimension**: Resource Lifecycle
- **Location**: `crates/renderer/src/vulkan/scene_buffer/buffers.rs:189` and `:995`; `crates/renderer/src/vulkan/buffer.rs:138-146` + `:329-331`; consumed at `crates/renderer/src/vulkan/scene_buffer/descriptors.rs:376`; hazard lands at `crates/renderer/src/vulkan/context/teardown.rs:408-446`
- **Status**: NEW (related to the un-numbered ROADMAP known issue "The engine SIGSEGVs at process teardown", `ROADMAP.md:1297-1319` — see honest caveat below)
- **Description**: `StagingPool` stores its allocator as a **bare** `allocator: SharedAllocator` (`buffer.rs:145`), not the `Option<SharedAllocator>` that every sibling GPU-resource type uses (`GpuBuffer`, `Texture`, `GpuImage`, `WaterPipeline`, `ExposureResource`). Its `destroy()` is just `self.trim_to(0)` — it frees pooled buffers but structurally *cannot* drop the `Arc` clone. `SceneBuffers` owns one such pool **non-optionally**, and `VulkanContext::scene_buffers` is a plain (non-`Option`) field, so that clone stays alive until the struct itself drops — after `Drop::drop` returns, i.e. after the `Arc::try_unwrap` in `teardown.rs`. The two sibling staging pools (`TextureRegistry::staging_pool` #732, `MeshRegistry::geometry_staging_pool` #1055) were both already given the `Option`+`take()` fix; the third pool, added by #3664 (2026-09-03, "make terrain tile uploads incremental"), was not.
- **Evidence**:
  - `buffer.rs:145` — `allocator: SharedAllocator,` (bare field).
  - `buffer.rs:329-331` — `pub fn destroy(&mut self) { self.trim_to(0); }` — no allocator release.
  - `buffers.rs:995` — `terrain_tile_staging_pool: StagingPool::new(device.clone(), allocator.clone()),`.
  - `context/mod.rs:1583` — `pub scene_buffers: scene_buffer::SceneBuffers,` — never `take()`n.
  - `buffer.rs:1888-1906` (#927's own test doc) states the regression check is "the absence of the 'outstanding references' error log on engine shutdown" — a check this pool makes permanently unsatisfiable.
  - `ROADMAP.md:1297-1307` records that log at **100% incidence** (75/75 bench runs at HEAD).
- **Impact**: `Arc::try_unwrap` fails → `teardown.rs:435-443` logs "GPU allocator has N outstanding references", fires `debug_assert!(false, …)` (a panic inside `Drop` on every debug-build exit), and returns early — **deliberately leaking the `VkDevice`, `VkSurfaceKHR`, `VkInstance`, and debug messenger** rather than doing a clean teardown. The leak-guard's own stated fallback expectation ("the OS reaps the leaked handles at process exit") is what keeps this HIGH rather than CRITICAL — but it means the engine has no clean teardown path at all, the validation layer's destroyed-device-with-live-objects check never runs, and every future real outstanding-reference regression is now masked by this permanent one.
- **Trigger Conditions**: Every `VulkanContext` drop, unconditionally — the pool is constructed on all paths with no feature gate.
- **Verification Path**: `cargo run` (debug build) and observe the `debug_assert!(false, …)` panic on exit, or grep the shutdown log for "outstanding references" (currently 100% incidence per ROADMAP). Not gated behind BYRO_VALIDATION — this is a plain Rust `Arc` refcount check, always observable.
- **Related**: #927 (the `Option<SharedAllocator>` mechanism + its regression test), #1055 (`MeshRegistry` fix), #732/LIFE-N1 (`TextureRegistry` fix), #665/LIFE-L1 (the leak-guard branch this trips), #1477/#1640 (the app-side `AllocatorResource` removal this defeats), #3664 (introduced the pool). **Honest caveat**: #3664 (2026-09-03) predates the clean control commit `e6282349` (2026-09-07) that segfaulted 0/30 runs, so this defect alone does not explain the *segfault* regression — the leak path is designed to exit "cleanly" (if noisily). It does fully explain the outstanding-references log, and it means that log can no longer be used as the regression signal #927 intended — fixing this is a prerequisite for bisecting the actual segfault.
- **Suggested Fix**: Change `StagingPool::allocator` to `Option<SharedAllocator>` and have `destroy()` `take()` it after `trim_to(0)` (matching `GpuBuffer::destroy`'s contract), fixing all three pools at the type level. Narrower alternative: make `SceneBuffers::terrain_tile_staging_pool` itself an `Option<StagingPool>` and `take()` it in `SceneBuffers::destroy`. Either way, add a source-shape pin (next to #927's `option_arc_dropped_when_set_to_none`) asserting no owned `StagingPool` field is reachable from `VulkanContext` without a `take()` in its owner's `destroy`, so a fourth pool can't reintroduce this a third time.

### CONC-D6-02: the teardown doc's "four load-bearing orderings" list includes one the code does not implement and does not need
- **Severity**: LOW
- **Dimension**: Resource Lifecycle
- **Location**: `crates/renderer/src/vulkan/context/teardown.rs:20-26` and `:151-166`
- **Status**: NEW
- **Description**: The corrected teardown header names ordering (b) as "placeholders after the passes whose descriptors name them," alongside three orderings that are real VUID-level constraints. The code does not implement it: `placeholder_ao`/`placeholder_caustic_sink` are destroyed at `teardown.rs:161-166`, **before** composite/caustic/volumetrics/bloom/water_caustic_accum/svgf/taa/gbuffer — several of which hold descriptor sets naming those placeholder views. The site comment at `:151-160` actually justifies the position by `device_wait_idle`, not by ordering — consistent with Dimension 6's own corrected model that Vulkan imposes no such cross-subsystem ordering once `device_wait_idle` has run.
- **Evidence**: placeholder destroys at `:161-166` vs. pass destroys at `:184-219`; the site's own comment contradicts the header's claim.
- **Impact**: No runtime hazard. The cost is to the invariant's credibility: a documented "load-bearing ordering" the code visibly violates trains readers (and the next audit) to discount the whole list, including the three entries that *are* load-bearing.
- **Trigger Conditions**: N/A — documentation/invariant drift.
- **Related**: #3658/CONC-D6-2026-08-30-02 (the correction that produced this list), #2141/#2142 (the placeholders).
- **Suggested Fix**: Demote (b) from the header list to "placeholders are allocator-backed, so must be destroyed before `self.allocator.take()` — their position relative to the passes naming them is unconstrained post-`device_wait_idle`," leaving three genuinely load-bearing orderings; reword the `:151-160` comment to match.

---

# Dimension 7 — Worker Threads (Streaming, Debug Server) & Thread-Safety Bounds

Scope: `byroredux/src/streaming.rs`, `crates/debug-server/src/{listener,system}.rs`,
`crates/renderer/src/vulkan/allocator.rs` + `egui_pass.rs`,
`byroredux/src/asset_provider/`, `merge_external_material`, `crates/ui/src/player.rs`,
`crates/cxx-bridge/`.

All six checklist items verified **clean**:
1. Streaming Drop ordering (#1167) — `WorldStreamingState::shutdown` still takes `worker` out first, then `request_tx`, then joins; `Drop` delegates entirely to `shutdown`.
2. Worker↔main data flow — payload crosses via `mpsc`; `PartialNifImport` has a compile-time `assert_send` guard; the worker only touches a read-only `Arc<HashSet<String>>` snapshot of the NIF import cache, never the live `NifImportRegistry`; `merge_external_material` is called only from main-thread cell-apply paths, never from the worker's parse functions; archive `File` access is `Mutex`-wrapped in both `BsaArchive`/`Ba2Archive`.
3. Debug server — per-client threads never touch `World`; all mutation routes through `DebugDrainSystem` (Late-stage exclusive); the command queue is bounded (`MAX_QUEUED_COMMANDS=64`) with a separate connection cap (`MAX_CONCURRENT_CLIENTS=8`, #3449); screenshot readback is fence-gated and sequenced before the drain system reads it, both on the main thread.
4. Allocator sharing — every `SharedAllocator::lock()` site locks only for the `allocate`/`free` call itself, released before any submit/wait.
5. Send/Sync bounds — no `unsafe impl Send`/`Sync` anywhere in the tree; `SwfPlayer` stays main-thread by construction (no `thread::spawn` call site ever touches it).
6. `material_translate.rs::translate_material`/`Material::resolve_pbr` confirmed single-threaded (out of scope, noted only).

### CONC-D7-01: `GpuImage` allocate path panics on a poisoned allocator mutex while its own free path recovers from it
- **Severity**: LOW
- **Dimension**: Worker Threads (Streaming, Debug) / Allocator sharing
- **Location**: `crates/renderer/src/vulkan/image.rs:188-190` (alloc path) vs. `crates/renderer/src/vulkan/image.rs:311-324` (`GpuImage::free_allocation`, used by both `destroy()` and `Drop`)
- **Status**: **Existing: #4089** (OPEN) — re-confirmed still present, not a regression
- **Description**: `GpuImage::new` locks the allocator with `.lock().expect("allocator lock")` (panics the calling thread if the mutex is poisoned), while `GpuImage::free_allocation` — documented as the single free-side lock-taking helper (#1163) — treats a poisoned mutex as recoverable via `poisoned.into_inner().free(...)`. The same type states two different poison-handling policies for the same lock.
- **Evidence**:
  ```rust
  // image.rs:188-190 (allocate)
  let allocation = match allocator.lock().expect("allocator lock").allocate(&desc) { .. }
  ```
  ```rust
  // image.rs:311-324 (free)
  fn free_allocation(allocator: &SharedAllocator, allocation: vk_alloc::Allocation, name: &str) {
      match allocator.lock() {
          Ok(mut guard) => { let _ = guard.free(allocation); }
          Err(poisoned) => { let _ = poisoned.into_inner().free(allocation); }
      }
  }
  ```
- **Impact**: If any allocator-holding thread ever panics while holding the lock (the `Mutex` is shared across the render thread, `EguiPass`, volumetrics, ssao, scene_buffer, and the texture/mesh registries), the next `GpuImage::new` call panics instead of degrading, while a concurrent `destroy()`/`Drop` on a different `GpuImage` would have recovered. Same blast radius as when #4089 was filed — not worsened, not fixed.
- **Trigger Conditions**: A panic anywhere while the shared allocator mutex is held, followed by any subsequent `GpuImage::new` call on the same allocator.
- **Related**: #4089 (OPEN), #1163 (the free-side single-site rule this partially violates), #1128/REN-D4-NEW-01.
- **Suggested Fix**: Route the allocate call through the same poison-recovery policy as `free_allocation` (`allocator.lock().unwrap_or_else(|e| e.into_inner())`), or invert the decision and make `free_allocation` also treat poison as fatal — whichever policy #4089's eventual fix settles on, apply to both call sites so `GpuImage` states the rule once.

---

## Suggested next step

```
/audit-publish docs/audits/AUDIT_CONCURRENCY_2026-09-11.md
```
Domain labels: `sync` for the Vulkan-side findings (Dimensions 1, 2, 6), `concurrency`
for the CPU-side ECS/scheduler findings (Dimensions 3, 4, 5, 7).
