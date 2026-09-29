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

1. Parse `$ARGUMENTS`. 2. `mkdir -p /tmp/audit/concurrency`.
3. `gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json number,title,state,labels > /tmp/audit/concurrency/issues.json`

## Phase 2: Launch Dimension Agents

### Dimension 1: Vulkan Queue & Acceleration-Structure Sync (CRITICAL surface)
Paths: `crates/renderer/src/vulkan/{sync.rs,texture.rs}`, `crates/renderer/src/vulkan/context/{init,draw,sync_and_acquire_frame,resize,skinned_blas_refit}.rs`, `crates/renderer/src/vulkan/acceleration/`
First step: `git log --since=<last-report-date> --format='%h %s' -- crates/renderer/src/vulkan/{sync.rs,acceleration,context}`
**Checklist**:
- **Queue submission is single-Mutex.** `graphics_queue` / `present_queue` are `Arc<Mutex<vk::Queue>>`
  (created in `context/init.rs`; `present_queue` is an `Arc::clone` of the graphics queue when the
  families match, so one Mutex serialises submits + presents). Vulkan requires external synchronization
  (`VUID-vkQueueSubmit-queue-00893`): bind the `MutexGuard` and keep it across `queue_submit` /
  `queue_present` even though `vk::Queue` is `Copy` (a temporary `.lock().unwrap()` drops at end of
  statement — `draw.rs` documents this at the submit). Release before any `wait_for_fences`. Also check the
  one-time-command submit in `texture.rs` (`one_time_lock_scope_tests` pins lock → submit → unlock → wait, #1713).
- **Frame-in-flight discipline (the both-slots wait).** `sync_and_acquire_frame.rs` waits on all
  `in_flight` fences before re-recording; that is the safety argument for the immediate scratch free in
  `build_skinned_blas_batched_on_cmd`, the TLAS resize, and every non-per-FIF resource. It is
  device-idle-equivalent only at `MAX_FRAMES_IN_FLIGHT == 2`. Guard: the const-assert in `sync.rs`
  (#870) and `frames_in_flight_contract_names_every_dependent_resource` (#3643) — confirm neither
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
  the staging slot is not rewritten while its frame is in flight.
**Output**: `/tmp/audit/concurrency/dim_1.md`

### Dimension 2: Compute → AS → Fragment Chains
Paths: `crates/renderer/src/vulkan/{skin_compute,svgf,taa,caustic,water_caustic,volumetrics,bloom,groundcover,sky_cube,material}.rs`, `crates/renderer/src/vulkan/context/{post_passes,dispatch_skin_and_cluster,skinned_blas_refit}.rs`
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
  stays here. `groundcover.rs::record_scatter` orders counter zero-fill → extrema seeds (TRANSFER→TRANSFER,
  #4293), scatter → publish incl. counter readback (#4181), and the interaction field's trailing barrier;
  `sky_cube.rs::record_bake` runs mid-frame (`context/build_and_upload_instances.rs`) — check its
  publish barrier before the first consumer. Evidence is a `BYRO_VALIDATION` capture, not reading.
- **Ground-cover model tier** (`groundcover_models.rs`, #4413): its compute writes `GpuInstance`s into the
  *tail of the main instance buffer* after `build_and_upload_instances` and before the geometry pass, which
  draws them with indexed-indirect — COMPUTE_WRITE must reach both `VERTEX_SHADER` (instance read) and
  `DRAW_INDIRECT` (args), and the host upload of the frame's own instances must not race the tail.
- **Exposure meter** (`exposure_meter.rs`): post-bloom composite → `exposure_meter.comp` → this frame's
  `ExposureResource` slot, sampled unconditionally by `presentation.frag` and the FSR dispatch — the slot
  write must be published before both consumers, in fixed mode too.
- **MaterialBuffer SSBO.** upload is `HOST_WRITE → VERTEX/FRAGMENT_READ`, before draw recording; flag only if it moves into a mid-frame compute path.
- Known-open 2026-09-29: #4989 (narrowed palette dispatches write one SSBO back to back with no barrier) and
  #4780 (the #3685 skip-clear latch also skips the temporal reset, leaving `history_valid` true).
**Output**: `/tmp/audit/concurrency/dim_2.md`

### Dimension 3: ECS Lock Ordering & Deadlock (system level)
Paths: `crates/core/src/ecs/{world,lock_tracker}.rs`, `byroredux/src/systems/`, `byroredux/src/extensions/`, `.github/workflows/ci.yml`
First step: `BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux` — the binary's graph is where every recent cycle lived (#4982–#4984; core alone stays green) — then CI's `lock-order-check` form, `cargo test --workspace --no-fail-fast --exclude byroredux-ui`. A nonzero failure count is a hard regression
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
  `byroredux/src/scheduler_access_tests.rs`). Known-open 2026-09-29 (#4987): that lane has not yet been seen to
  reach a device — read the run log for a device-selection line before counting live-batch coverage. A green
  run proves only what it exercised.
- **The graph cannot tell `&mut World` from `&World`.** A function holding several read guards at once under
  `&mut World` cannot deadlock but still records edges and reddens the lane (#4982, the unload capture
  passes). The fix is the same snapshot-then-acquire shape, not an exemption.
- **Canonical order.** `docs/engine/ecs.md` § Lock-ordering policy is the arbiter for hand-ordered
  holds (`StringPool` is a sink: acquired last, nothing beneath it). New multi-lock code follows it.
- **Guard lifetime in system bodies.** No `query_mut` / `resource_mut` guard held across a call that
  re-enters the same storage/resource or acquires a pair the other way; nested patterns drop the first guard or
  use `query_2_mut`; no structural mutation (`World::insert` / `spawn` need `&mut self`) inside a system.
- **Guard ↔ sandbox boundary.** `byroredux/src/extensions/` hosts untrusted WASM behind an
  `Arc<Mutex<ExtensionHost>>` (`ExtensionHostSlot`, `extensions/systems.rs`). Every dispatch path snapshots
  ECS state before guest entry, drops every ECS guard, enters the guest, and commits its returned command
  batch afterwards (`extensions/dispatch.rs` module doc; `commands.rs::enter_guest`). A host `Mutex`
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
  (≥9 parallel systems, ≥7 pairs) still hold, and `PARALLEL_SYSTEMS.len()` equals the `add_to_with_access(` count.
- **Blind spots of the guard** (audit by hand): the scan reads turbofish `query` / `query_mut` / `resource` /
  `resource_mut` (+ `try_`), `world.get` / `get_mut` / `has` and generic `remove_component` /
  `insert_component` (#4994, #4821) — not `query_2_mut`, `resource_2_mut` or inferred types; it follows calls
  within a listed file but cross-file hops only where `PARALLEL_SYSTEMS` lists them (#4994,
  `cross_file_hops_and_get_forms_reach_their_acquisitions`); closures and macros; exclusive systems (three are
  scanned — `npc_combat_ai_system`, `papyrus_provider_system`, `legacy_obscript_load_order_system`; others only
  by the non-empty-access tests).
  An under-declared parallel system makes `known_conflict_count() == 0` unsound (the same-session
  `fly_camera_system` `GlobalTransform` write is the precedent, commit ac1d44f5c).
- **Cross-stage sequencing is invisible to the analyzer** (`analyze_pair` reasons within one stage). A
  consumer in an earlier stage than its single producer silently reads last frame's value while every KPI
  stays green. Pinned today: `player_wind_read_is_declared_and_weather_writer_is_exclusive` (WindField:
  weather is an Early exclusive registered after the parallel player controller, so the controller reads
  the previous frame's wind by design, #3111/#4186), `billboard_runs_after_camera_follow_in_late`,
  `footstep_runs_after_camera_follow_in_late`, `submersion_runs_after_camera_follow_and_before_water_audio`
  (#3652/#3180/#4185 — each was a real one-frame-stale bug), `player_body_facing_runs_in_update_before_propagation`
  (#4995). For any NEW single-writer / multi-reader resource,
  check writer-stage ≤ reader-stage and that a test like these pins it; fix by moving the *consumer* to
  a Late exclusive after the writer, not by moving the writer.
- **Exclusives** run serially after the parallel batch and are never paired; undeclared ones are by design (a
  system demoted to exclusive to clear a conflict stays exclusive). Flag only a *parallel* system that lost its declaration.
**Output**: `/tmp/audit/concurrency/dim_4.md`

### Dimension 5: RwLock Patterns — Resource↔Storage & Physics Step
Paths: `crates/physics/src/{sync,world,components,config}.rs`, `byroredux/src/cell_loader/unload.rs`, `byroredux/src/systems/character.rs`, `byroredux/src/ragdoll.rs`
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
  `get_actor_value_does_not_hold_actor_values_across_ruleset` (`crates/scripting/src/condition.rs`,
  `CharacterRuleset -> ActorValues`), `unload_captures_do_not_invert_production_lock_orders` and
  `picked_up_restore_walks_meshes_before_taking_the_marker` (`byroredux/src/cell_loader/reference_state.rs`,
  #4982/#4983), `view_restamp_does_not_close_the_render_skip_lock_cycle` (`byroredux/src/player_body.rs`,
  #4991). The count of closed cycles is open and growing — the CI
  lock-order job (Dim 3) is what finds the next one; a second site acquiring `StringPool` mid-graph is the same class.
- **Helper order.** `set_linear_velocity` / `set_kinematic_translation` read `RapierHandles` via
  `world.query::<RapierHandles>()…copied()` (guard drops with the expression), *then* take
  `resource_mut::<PhysicsWorld>()`; callers (e.g. `character_controller_system`) must not already hold a
  `PhysicsWorld` guard.
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
- **Swapchain recreate.** G-buffer, SVGF, TAA, caustic, water-caustic, volumetrics, bloom, composite and
  egui framebuffers are rebuilt; per-FIF history/accumulator images freed for every in-flight slot (`resize.rs`).
- **AS cleanup on shutdown.** All `BlasEntry` buffers, `TlasState` buffers and scratch released; per-entity skin outputs kept until the entity is destroyed.
- **Other GPU cleanup.** `scene_buffer`, `MaterialBuffer`, texture registry (incl. the `dynamic_rgba` staging
  arena), `EguiPass::destroy()` (`Option<EguiPass>` taken in `Drop`; its texture retirements are a
  frames-in-flight rider, #4988), a partially-built object's error arm (`GpuPerFrameTimers::new`, #4998),
  and `GpuImage` (`vulkan/image.rs`), which routes most passes' image
  create/bind/destroy through one lock rule: a poisoned allocator lock is recovered, not unwrapped (#4089).
- **Per-frame leaks.** Any descriptor / command-buffer / staging allocation created per frame but not freed or reset is HIGH.
**Output**: `/tmp/audit/concurrency/dim_6.md`

### Dimension 7: Worker Threads & Thread-Safety Bounds
Paths: `byroredux/src/streaming.rs`, `byroredux/src/asset_provider/texture_prefetch.rs`, `byroredux/src/cell_loader/load_order.rs`, `byroredux/src/render/mod.rs`, `crates/bsa/src/read_at.rs`, `crates/debug-server/src/{listener,system}.rs`, `crates/renderer/src/vulkan/allocator.rs`, `crates/ui/src/player.rs`, `crates/audio/src/lib.rs`
First step: `grep -rnE 'thread::(spawn|Builder)|rayon::|mpsc::' --include='*.rs' crates byroredux tools | grep -v test` (a new worker thread outside this list is a coverage gap)
**Checklist**:
- **Streaming worker shutdown.** `WorldStreamingState::shutdown` takes the `worker` handle first, then
  drops `request_tx` (the worker's `recv()` errors and it exits), *then* `join_with_timeout` (poll on
  `is_finished`, no watcher thread); `Drop` only calls `shutdown(1 s)` as a safety net, so a prior explicit
  shutdown short-circuits it. The worker runs each cell under `catch_unwind`
  (`pre_parse_cell_panic_safe`).
- **Worker ↔ main flow.** Parsed payloads move to the main thread over a channel; no shared `&mut World`.
  The worker parses on its own rayon pool (`build_stream_parse_pool`, sized so it does not starve the
  scheduler's global pool) and uses `Arc<TextureProvider>`; BSA/BA2/CSG extraction is lock-free positional
  reads (`crates/bsa/src/read_at.rs`, `ReadAt`, commit 1b8b21f3f — concurrent on Unix, kernel-serialised on
  Windows, #4999). A shared-cursor seek+read reintroduced anywhere returns interleaved wrong bytes (threaded tests per archive, #5000).
  Streamed NIFs' textures are prefetched on the stream pool (`prefetch_textures`, `PrefetchStore`): a panicking
  read must still complete its slot, and a withdrawn key must not be waited on. External-material
  resolution (`merge_external_material`, which takes `&mut ImportedMaterial` + `&mut MaterialProvider` +
  `&mut StringPool`) is main-thread-only — moving it needs a real synchronisation story. The NIF import
  cache is read-only on the worker with write-back deferred to main.
- **Debug server (thread/lock shape only; command surface is `/audit-tooling`).** Per-client TCP threads
  never touch the `World`: they enqueue into a bounded queue (`MAX_QUEUED_COMMANDS`, client cap
  `MAX_CONCURRENT_CLIENTS`, `listener.rs`) and `DebugDrainSystem` (Late exclusive) executes on the main
  thread. Screenshot readback completes on a fence wait — check the drain/present race.
- **Allocator sharing.** `SharedAllocator = Arc<Mutex<Allocator>>` is cloned into the egui pass,
  volumetrics, SSAO, scene buffers, etc.; no holder keeps it locked across a queue submit or a
  fence wait. The egui pass takes the queue as a `Mutex` so its lock scopes to the `set_textures` submit (#1713).
- **`Send + Sync` bounds.** Component/Resource storage is reached only through World guards; no raw
  pointer crosses threads; the Ruffle/wgpu device (`crates/ui`) is `Send` but not `Sync` and stays on one
  thread; kira runs its own audio thread behind `AudioWorld` — no ECS guard is held across a kira call.
- **Other rayon fan-outs.** `build_render_data` (`byroredux/src/render/mod.rs`) runs nested `rayon::join`
  branches that take ECS guards on pool threads — every branch must be read-only, or a write in one branch vs.
  a read in another is a cross-thread ABBA no CI lane drives. The plugin load-order walk
  (`cell_loader/load_order.rs`, `rayon::in_place_scope`, #3813) walks plugins in parallel; the `merge_from`
  fold must follow load order, never completion order.
- Out of scope: parse-time `Material` translation is single-threaded (`/audit-nifal`).
**Output**: `/tmp/audit/concurrency/dim_7.md`

## Phase 3: Merge

1. Read all `/tmp/audit/concurrency/dim_*.md`. 2. Combine into `docs/audits/AUDIT_CONCURRENCY_<TODAY>.md`. 3. Remove cross-dimension duplicates.

Suggest: `/audit-publish docs/audits/AUDIT_CONCURRENCY_<TODAY>.md` (domain label *concurrency* for CPU-side
lock ordering / access declarations, *sync* for GPU-side semaphore/fence/barrier findings).
