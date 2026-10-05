**HEAD**: `a2c24b16e` · **Baseline**: `docs/audits/AUDIT_CONCURRENCY_2026-09-29.md` (@ `9fcfdc3fc`, 313 commits ago) · **Audited**: Dims 1–7 (every dimension's `Paths:` had commits: D1 ~45, D2 ~55, D3 30, D4 11, D5 16, D6 ~40, D7 14 — mostly the #4880–#5246 renderer / physics fix waves, P4 dialogue, #5146 footsteps, LSCR loading cover) · **Unchanged since baseline (skimmed)**: none at the dimension level. Sub-paths with zero commits, guard spot-checked only: `crates/core/src/ecs/{world,lock_tracker,access}.rs`, `crates/physics/src/{components,config}.rs`, `crates/audio/src/lib.rs`, `crates/bsa/src/read_at.rs`, `byroredux/src/asset_provider/texture_prefetch.rs`.

# Concurrency & Synchronization Audit — 2026-10-05

**Command**: `/audit-concurrency` (all dimensions, depth `deep`), run inside `/audit-suite --preset comprehensive`.
**Severity scale**: `.claude/commands/_audit-severity.md`.

## Method

- **Delta scope.** `git log 9fcfdc3fc..HEAD -- <Paths>` per dimension. All seven dimensions were analysed synchronously in this
  session (no sub-agents). Per-dimension notes are in `/tmp/audit/concurrency/dim_{1..7}.md`, and this report was reconciled
  against each of them.
- **No engine launch** (suite rule). The Vulkan evidence comes from the CI `vulkan-validation` job logs, bisected across 21 main
  runs since the baseline.
- **Dedup.** Sources checked:
  - open issues (`/tmp/audit/issues.json`, 97);
  - closed-issue searches for each finding;
  - today's sibling report `AUDIT_ECS_2026-10-05.md`;
  - the 09-29 concurrency report.

### Guard runs at HEAD (local, rustc 1.96, `TMPDIR=/mnt/data/tmp`)

| Command | Result |
|---|---|
| `BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux --no-fail-fast` | bin: 2643 passed, **5 failed**, 52 ignored. The failures are `systems::character::tests::camera_follow_does_not_close_character_lock_cycle` and four `systems::water::tests::*`, all lock-order-cycle panics at `lock_tracker.rs:476`. This is exactly ECS-2026-10-05-D1-01. |
| `BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux-scripting` | 502 passed, 0 failed |
| `cargo test -p byroredux --bin byroredux -- scheduler_access system_access_declaration` | 30 passed, 0 ignored |
| `cargo test -p byroredux-physics sync` | 31 passed |
| `BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux-physics` | 206 passed |
| `cargo test -p byroredux-renderer --lib -- frames_in_flight one_time_lock_scope partial_delta_promotion skin_publish_barrier taa_resolves render_finished rgba_updates only_the_submitted gpu_timers dependency_chain caustic restir reservoir every_skipped_frame occupancy may_be_in_flight scratch egui teardown lock_recovering` | 161 passed |
| `cargo test -p byroredux-bsa --lib -- concurrent threaded parallel` | 3 passed |
| CI run 37341506292 (`23524b446`) | The "ABBA lock-order detector" job fails on the ECS D1-01 cycles. The "Vulkan validation layers (lavapipe)" job fails too, but its output contains **zero errors** (CONC-D3-2026-10-05-01). |

## Summary

| Severity | NEW | Regression | Existing |
|---|---|---|---|
| CRITICAL | 0 | 0 | 0 |
| HIGH | 0 | 0 | 0 |
| MEDIUM | 2 | 1 | 0 |
| LOW | 1 | 0 | 2 (#5066, #5069) |

Cross-referenced, not re-filed: **ECS-2026-10-05-D1-01** (HIGH, owned by today's ECS report). In `footstep_system`,
`byroredux/src/systems/audio.rs:175-226` (`ccc743160`, #5146), takes `CharacterController` and `WaterContact` while it holds the
`FootstepScratch` / `GlobalTransform` / `FootstepEmitter` guards. That closes three production cycles, and the ABBA lane is red.

This audit reproduced the cycles independently and found nothing beyond them:
- The detector reports exactly the five panics that report lists, and the scripting graph is green.
- The footstep body also holds the `CharacterController` guard across the `WaterContact` get. That adds a
  `CharacterController → WaterContact` edge, but `character_controller_system` (`character.rs:313`) records the same edge in the
  same direction, and no reverse edge exists.

**Headline.**
- **Both of the 09-29 HIGHs are fixed and confirmed live.** #5062 (ReSTIR reservoir RAW) and #5064 (caustic bindings 9/10,
  VUID-08114) produced zero `SYNC-`/`VUID-` lines on every main run from 09-30 onward. #5072 (the baseline D6-01 egui re-seed) and
  #4780 are also fixed.
- **The only live Vulkan-sync gate is red for the wrong reason, and it has quietly stopped covering RT.** Two separate regressions
  hit the `vulkan-validation` lane after the baseline:
  1. **Red on warnings since 10-01.** #4987's own fix (`6d5d8fa5f`) lifted the renderer crate to `info`, so WARN-level
     `WARNING-Shader-OutputNotConsumed` performance warnings now trip the `[Vulkan]` grep. The lane has been red on every run
     since, with no validation error in any of them (CONC-D3-2026-10-05-01).
  2. **RT off in the lane since 10-03.** #5188 (`d59f3c7a4`) drops RT whenever the global geometry SSBO is absent, and the CI demo
     scene never builds one. Since 10-03 the lane runs with `rt_flag=0 tlas_build=0`, so it no longer exercises the fragment ray
     queries, the caustic splat or the volumetrics TLAS path. Those are exactly the consumers where the last two HIGHs were found
     (CONC-D2-2026-10-05-01).
- **#5195 missed one sibling.** The TLAS scratch is now *allocated* at max(build, update), but the shrink peak still records the
  build size alone. On a driver whose `updateScratchSize` is larger than its `buildScratchSize`, the post-present shrink undoes the
  fix (CONC-D1-2026-10-05-01).

| ID | Sev | Dim | Status | Title |
|---|---|---|---|---|
| CONC-D1-2026-10-05-01 | MEDIUM | D1 | Regression of #5195 | The TLAS scratch shrink peak still records `build_scratch_size` alone, so `shrink_tlas_scratch_to_fit` can cut the scratch below `updateScratchSize` |
| CONC-D2-2026-10-05-01 | MEDIUM | D2 | NEW | #5188's geometry-dead arm turns RT off for the CI demo scene, so the validation lane no longer exercises any RT consumer |
| CONC-D3-2026-10-05-01 | MEDIUM | D3 | NEW | The `vulkan-validation` lane is red on WARN-level `OutputNotConsumed` performance warnings since #4987's fix, with zero validation errors |
| CONC-D1-2026-10-05-02 | LOW | D1 | NEW | FIF rider 8 still points at `groundcover.rs`'s `prepare`; #5089 moved it to `groundcover/frame.rs`, and the contract test does not pin rider 8 |
| — | LOW | D3 | Existing: #5066 | The `npc_dialogue` `LoadedCellIndex` guard shadow is still present. Since #5152 the guard also spans the "guard-free" INFO fragment apply (status update) |
| — | LOW | D4 | Existing: #5069 | The `equipment_appearance_system` Access row is still missing the gear-import surface (unchanged) |

---

## Findings

### CONC-D1-2026-10-05-01: The TLAS scratch shrink peak still records `build_scratch_size` alone, so `shrink_tlas_scratch_to_fit` can cut the scratch below `updateScratchSize`
- **Severity**: MEDIUM. This is the same grading as #5195: a latent spec violation that needs a driver where
  `updateScratchSize > buildScratchSize`, and none has been observed. On such a driver the next TLAS UPDATE refit overruns the
  scratch buffer (VUID-vkCmdBuildAccelerationStructuresKHR-pInfos-12259), which is a GPU write past the allocation. Escalate to
  CRITICAL if a dump of the 4070 Ti or RADV values shows the premise failing.
- **Dimension**: Vulkan Queue & AS Sync
- **Location**:
  - The peak write: `crates/renderer/src/vulkan/acceleration/tlas.rs:1151`, with its stale comment at `:1139-1150`.
  - The shrink: `crates/renderer/src/vulkan/acceleration/memory.rs:361-385` (`shrink_tlas_scratch_to_fit`), with its doc at `:293`.
  - The caller: `crates/renderer/src/vulkan/context/draw.rs:783`, post-present.
  - The pin: `crates/renderer/src/vulkan/acceleration/tests/scratch_tests.rs:610`.
- **Status**: Regression of #5195 (closed by `640a3039d`). This is an incomplete fix: #5195's "Related" line named the TLAS sibling,
  and the commit changed only the TLAS *allocation*, not the peak.
- **Verification Path**: Not visible to `cargo test`. The validation layer reports it only on hardware where the premise fails, so the
  cheapest evidence is a one-line log of `sizes.update_scratch_size` vs `sizes.build_scratch_size` at the `ensure_tlas_state` query.
- **Description**: #5195 established that the spec does not relate the two scratch sizes, and it made `ensure_tlas_state` allocate
  `max(build, update) + padding`. The peak that drives the shrink was left as the build size:
  ```rust
  // tlas.rs:1139-1151
  // ... refit/update reuse the existing scratch on
  // the spec guarantee `BUILD ≥ UPDATE`. So this is the
  // canonical peak for the slot's lifetime, ...
  self.tlas_scratch_peak_bytes[frame_index] = sizes.build_scratch_size;
  ```
  The shrink then reallocates to `peak + scratch_alignment_padding`, that is, the build size plus padding (`memory.rs:384`).

  If `update > 2 × build + 256 KB` (`tlas_scratch_should_shrink`), the shrink fires on the very next post-present tick after every
  fresh build. It replaces a correctly sized scratch with one that is too small for UPDATE.

  The next frame on that slot fits the existing TLAS, so `ensure_tlas_state` returns early without re-growing scratch, and
  `decide_use_update` picks UPDATE. In milder cases the same thing happens after a shrink-triggered rebuild lowers the recorded peak.

  The source pin `fresh_build_records_peak_unconditionally_of_scratch_regrow` matches the literal
  `= sizes.build_scratch_size;`, so it currently pins the defect.
- **Trigger Conditions**: A driver with `updateScratchSize > buildScratchSize`, RT on, and a TLAS slot that reaches the post-present
  shrink (immediately, if update is at least 2× build plus 256 KB).
- **Impact**: A GPU-side write past the scratch allocation, corrupting whatever the allocator placed next to it. That is TLAS-wide, so
  every ray query is affected. It is invisible on hardware where the premise holds.
- **Related**: #5195, #2915, #2774, #1386.
- **Suggested Fix**: Record `sizes.build_scratch_size.max(sizes.update_scratch_size)` as the peak, fix the `:1139-1150` comment and the
  `memory.rs:293` doc, and change the `scratch_tests.rs:610` needle to the new spelling.

### CONC-D2-2026-10-05-01: #5188's geometry-dead arm turns RT off for the CI demo scene, so the validation lane no longer exercises any RT consumer
- **Severity**: MEDIUM. There is no runtime defect in shipped content. The cost is that the CRITICAL-surface validation gate is now
  blind, which is the #4596 / #4987 "inert lane" class.
- **Dimension**: Compute → AS → Fragment Chains
- **Location**:
  - The RT drop: `crates/renderer/src/vulkan/context/dispatch_skin_and_cluster.rs:428-441` (`geometry_dead` →
    `tlas_written[frame] = false`, `patch_camera_rt_flag(.., 0.0)`, and the skipped `if !tlas_build_failed && !geometry_dead` arm).
  - The caustic gate close: `crates/renderer/src/vulkan/context/sync_and_acquire_frame.rs:302-310`.
  - The demo scene: `byroredux/src/scene.rs:820-854` (`spawn_demo_primitives`, which uploads with `MeshRegistry::upload`).
  - The non-global path: `crates/renderer/src/mesh.rs:562` (`upload`, which never reaches `accumulate_global_geometry`).
- **Status**: NEW. Searched "rt_flag=0", "geometry_dead", "demo scene RT" and "validation lane RT"; there were no matches.
- **Verification Path**: CI log (already captured below). After a fix, the lane's `rt-integrity:` line must read
  `rt_flag=1 tlas_build=1` again.
- **Description**: #5188 correctly stops RT shading through stale scene-set bindings 8/9 after a failed geometry rebuild. Its trigger,
  though, is `self.mesh_registry.global_vertex_buffer.is_none()`, and that is also the permanent state of the content-free demo scene.
  The baseline already noted that the demo primitives never enter the global pool. The CI `vulkan-validation` job runs exactly that
  scene.

  The TLAS is still built, and the AS_WRITE → AS_READ barrier is still recorded. But with `rt_flag = 0` and `tlas_written = false`,
  none of the following run any more:
  - the fragment ray queries;
  - the caustic splat (its latch is closed by the new else arm);
  - the volumetrics TLAS path;
  - RT GI.
- **Evidence**: The `rt-integrity:` line in the `vulkan-validation` job:

  | Run | Commit | Result |
  |---|---|---|
  | 37151756863 | `3cf32ee5a` | `rt_flag=1 tlas_build=1` |
  | 37155077588 | `be3cd9468` | `rt_flag=1 tlas_build=1` |
  | 37161409881 | `2c36c29d8` (first run containing `d59f3c7a4`) | `rt_flag=0 tlas_build=0` |
  | every later run through 37341506292 | `23524b446` | `rt_flag=0 tlas_build=0` |
- **Trigger Conditions**: Every CI run, every frame (the demo scene has no global geometry).
- **Impact**: A new barrier, descriptor or layout hazard on any RT consumer cannot surface in CI. #5062 and #5064 were both found only
  because the lane did reach those consumers on 09-29.
- **Related**: #5188, #5064, #4987, #4596, CONC-D3-2026-10-05-01.
- **Suggested Fix**:
  - Upload the demo primitives through `upload_scene_mesh` (the global pool), so the demo scene has live geometry globals and RT stays on.
  - Add a lane assertion that the `rt-integrity:` line carries `rt_flag=1 tlas_build=1`, so a silent RT-off fails the job instead of passing it.

### CONC-D3-2026-10-05-01: The `vulkan-validation` lane is red on WARN-level `OutputNotConsumed` performance warnings since #4987's fix, with zero validation errors
- **Severity**: MEDIUM. This is the same grading as #4987 and #4603: a permanently red gate masks the next real `VUID-*` /
  `SYNC-HAZARD-*`.
- **Dimension**: ECS Lock Ordering (the CI-lane half of Dim 3)
- **Location**: `.github/workflows/ci.yml:398` (`export RUST_LOG=error,byroredux_renderer=info`) and `:413` (the `[Vulkan]` grep).
  The warnings come from main-pipeline creation, logged right after "Graphics pipelines created (opaque early tests=true …)".
- **Status**: NEW. Searched "OutputNotConsumed" and "vulkan-validation lane"; the only hits are the closed #4596, #4987, #5062 and #5064.
  The skill's own Dim 3 text warns that WARN-level lines can redden the lane, but no issue tracks it.
- **Verification Path**: CI log (captured).
- **Description**: To let the positive `Selected GPU:` gate see `device.rs`'s info line, `6d5d8fa5f` raised the renderer crate from
  `error` to `info`. The debug messenger logs performance warnings at WARN with the `[Vulkan]` prefix. The gate's
  `grep -qF '[Vulkan]'` does not distinguish severities, so the 9–10 `WARNING-Shader-OutputNotConsumed` lines on every run fail the
  job. They report vertex outputs at locations 6, 7, 10–13, 15, 16, 20 and 21 that the fragment stage of the early-test variant does
  not consume.
- **Evidence**: The `vulkan-validation` job across main runs:

  | Runs | Commits | Result |
  |---|---|---|
  | 36712814522 … 36789466327 | `c9254beb8` … `01a85cfc4` (09-30) | **success**, zero messages |
  | 36908954333 | `4ad847a81` (first run containing `6d5d8fa5f`) | failure: 9 × OutputNotConsumed, 0 VUID, 0 SYNC |
  | every later run through 37341506292 | through `23524b446` | failure: 9–10 × OutputNotConsumed, 0 VUID, 0 SYNC, no `panicked at` |

  `6d5d8fa5f` is not an ancestor of `01a85cfc4`, and it is an ancestor of `4ad847a81`.
- **Trigger Conditions**: Every CI run since 10-01.
- **Impact**: For five days the lane's red status has carried no information. A reviewer must open each log and read which severity
  fired. Combined with CONC-D2-2026-10-05-01, the lane is now both red on a non-error and blind to RT.
- **Related**: #4987, #4603, #4596, CONC-D2-2026-10-05-01.
- **Suggested Fix**:
  - Scope the info lift to the device module only: `RUST_LOG=error,byroredux_renderer::vulkan::device=info`.
  - Or make the gate match error-severity messenger lines only. Pin whichever you choose next to
    `vulkan_validation_job_requires_a_selected_device`.
  - Separately, and optionally, trim the unconsumed varyings or accept them explicitly.

### CONC-D1-2026-10-05-02: FIF rider 8 still points at `groundcover.rs`'s `prepare`; #5089 moved it to `groundcover/frame.rs`, and the contract test does not pin rider 8
- **Severity**: LOW (doc rot plus a test gap in a load-bearing list).
- **Dimension**: Vulkan Queue & AS Sync
- **Location**: The rider: `crates/renderer/src/vulkan/sync.rs:78-82`. The moved function: `crates/renderer/src/vulkan/groundcover/frame.rs:16`.
  The test: `crates/renderer/src/vulkan/sync.rs:653-768` (`frames_in_flight_contract_names_every_dependent_resource`).
- **Status**: NEW
- **Description**: Rider 8 names "`groundcover.rs`'s `prepare` on `current_frame` (#4601)", which harvests `counter_readback[frame]`.
  `0f9982177` (#5089) split `groundcover.rs` into `groundcover/construct.rs` and `groundcover/frame.rs`, and `prepare` now lives in
  `frame.rs`. The contract test pins 15 resource needles, including rider 9's `groundcover_models` and rider 15's
  `combustion_occupancy_buffers`, but nothing for rider 8. So the move went through with the rider still pointing at a file that no
  longer contains the function. The block asks to be "re-derived rather than trusted", and a stale site makes that harder at exactly
  the moment it matters: a FIF bump or the #4606/#5117 wait narrowing.
- **Suggested Fix**: Repoint rider 8 to `groundcover/frame.rs`'s `prepare`. Add a `("counter_readback", production_text(include_str!("groundcover/frame.rs")))`
  entry to the contract test.

---

## Existing issues re-checked

- **#5066 (LOW, open). Status update, not a new finding.** `npc_dialogue.rs:507-510` and `:585-588` still shadow the `LoadedCellIndex`
  guard instead of dropping it. Since `c59600138` (#5152) the slip spans more code. Pass 2 now runs this chain with the guard alive:
  `apply_selection` → `speak_info_{begin,end}_fragment` → `byroredux_scripting::apply_spoken_info_fragment` →
  `apply_fragment_guard_free`. That last call takes `resource_2_mut::<QuestStageState, QuestObjectiveState>` + `apply_effects` +
  `DeferredFragmentEffects`, and its documented contract is a "guard-free apply". `select_topic_by_form_id` (the UI click, under
  `&mut World`) does the same. No cycle closes today: every other `LoadedCellIndex` acquisition copies the `Arc` and drops the guard,
  so there is no production `X → LoadedCellIndex` edge. The fix is unchanged. Suggest `/audit-publish` add this widening as a comment
  on #5066.
- **#5069 (LOW, open).** `late.rs:416-429` gained `.writes::<PendingGearRelease>()` (#5028) but still omits `ActorBodyClass`,
  `PendingGearImport` and `LoadedCellIndex`.
- **Verified fixed** since 09-29:
  - **#5062.** `ReservoirCurrBuffer` is now `writeonly`, and the after-barrier dst is `SHADER_WRITE | SHADER_READ`. Its lane hazard is gone.
  - **#5064.** The per-FIF `geometry_bound` latch gates the caustic dispatch. #5188 added its closing half. VUID-08114 is gone.
  - **#5072.** Mirrors are taken before destroy and re-seeded after the rebuild; a re-seed failure disables the overlay. Pinned.
  - **#4780.**
  - **#4987** (closed). Its premise holds, but see CONC-D3-2026-10-05-01.
  - **#5209.** The transfer fence and the one-time queue lock now use `lock_recovering`.
  - **#5201, #4883, #4884.** Unwinding and allocate-before-retire. No new immediate `destroy_acceleration_structure` at any eviction
    site; the new destroys are on never-recorded or never-submitted handles only.
  - **#5194.** Build-then-swap retires the old skinned BLAS through `pending_destroy_blas`.
  - **#5215.** Rider 15 is in place and pinned.
  - **#4890.**

## Dimension notes (clean areas)

- **D1 — Queue & AS.**
  - `draw.rs` binds the queue guard across `queue_submit` (dropped on both arms) and the present guard across `queue_present`.
  - The `MAX_FRAMES_IN_FLIGHT == 2` const-assert and the contract test are live and not ignored.
  - `dynamic_rgba` (#4889) degrades staging failures to a skipped overlay frame. Any copies already recorded keep
    `recorded_slot = Some(frame)` and are consumed only on submit.
- **D2 — Compute chains.**
  - Two #5187 skin-chain changes:
    - a residency gate before slot and BLAS creation;
    - a deferred-destroy swap.
  - The ground-cover model tier barrier set is unchanged: COMPUTE → DRAW_INDIRECT | VERTEX | FRAGMENT | TRANSFER.
  - The exposure-meter publish barrier is unchanged; the exposure commits change constants and shader math only.
  - The occupancy-mask device RAW still rests on `record_volumetrics_pass`'s global COMPUTE barrier.
- **D3 — ECS lock ordering.**
  - There are no cycles other than ECS D1-01.
  - `extensions/` changed in tests only.
  - The new exclusives (`loading_model_turntable_system`, `player_derived_stats_system`) each take a single guard.
- **D4 — Scheduler proof.**
  - Registration counts: 9 `add_to_with_access` = `PARALLEL_SYSTEMS.len()`, 38 exclusives-with-access, 46 bare exclusives.
  - `camera_follow_system`'s new `player_camera_boom` hop (#5124) reads `PlayerMode` and `PlayerCameraView`, and both are declared.
  - No new earlier-stage consumer reads a later-stage single writer.
- **D5 — Physics RwLock.**
  - All named snapshot-then-acquire guards are present and green.
  - The #5161 / #5246 containment lives inside `PhysicsWorld`.
  - `ragdoll.rs` activation and `release_victim_rapier_bodies` still collect under read guards and drop them before the
    `PhysicsWorld` write.
- **D6 — Lifecycle.**
  - Teardown changed only in the two poison-recovery swaps.
  - The new `combustion_occupancy_buffers` are created and destroyed symmetrically, including on the partial-init path.
- **D7 — Workers.**
  - No new thread, rayon section or channel.
  - The new Starfield CDB index cache (`asset_provider/material/cdb.rs:81-130`) scopes its lock to the lookup, builds outside the
    lock, is poison-recovered, and is reached from the main thread only.
  - The `build_render_data` join branches gained no write guard.
  - #5141's drain answers pre-cancelled commands without evaluating them (an Acquire/Release flag).

## Skill drift (fold into the next `/audit-concurrency` sync)

- **Dim 3 should check the lane's `rt-integrity:` line, not just its messages.** Add "`rt_flag=1 tlas_build=1` on the lane's
  `rt-integrity:` line" to the checklist next to the `Selected GPU:` note. A lane that reaches a device but runs RT-off proves nothing
  about Dims 1–2.
- **`crates/renderer/src/vulkan/restir.rs` is still in no dimension's `Paths:`.** The baseline already flagged this. Dim 2's First step
  glob covers it, but Paths does not.
- Dim 2's Paths list `groundcover/` correctly, but `sync.rs`'s rider list does not (CONC-D1-2026-10-05-02).

---

Publish with: `/audit-publish docs/audits/AUDIT_CONCURRENCY_2026-10-05.md`. Domain labels:
- **sync**: CONC-D1-2026-10-05-01, CONC-D2-2026-10-05-01 and CONC-D1-2026-10-05-02.
- **concurrency** + `test-gap`: CONC-D3-2026-10-05-01 (CI lane).
- Also post the #5066 widening as a comment on #5066.
