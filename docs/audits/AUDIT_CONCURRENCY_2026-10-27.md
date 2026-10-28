**HEAD**: `fa6be4a6b` · **Baseline**: `docs/audits/AUDIT_CONCURRENCY_2026-10-21.md` (@ `4a3e89d56`, 395 commits ago) · **Audited**: Dim 1 (#6143 driver split, #6250 moved-surface buffers, queue-guard sites), Dim 2 (#6249 shadow-filter mesh-ID read, ground-cover wind, underwater god rays), Dim 3 (three lock-order lanes re-run, CI ABBA job log grouped, new AI / crime / VATS system bodies), Dim 4 (proof plus the #5647 story-event hop), Dim 5 (PhysicsWorld-sink re-scan over every new site), Dim 6 (#6343 / #6146 / #6348 hardening), Dim 7 (new threads, #6407 parse cache) · **Unchanged since baseline (skimmed)**: none fully. Dims 1, 2 and 6 had commits but no new submit / wait / AS-destroy call and no new barrier edge, so they were checked by guard runs plus targeted reads.

# Concurrency & Synchronization Audit — 2026-10-27

**Command**: `/audit-concurrency` (all 7 dimensions, depth `deep`), inside `/audit-suite --preset comprehensive`. Run solo: no sub-agents, no source edits, no engine or GPU process. Built in the repo `target/` only.
**Dedup**: `/tmp/audit/open_issues_snapshot.txt` (84 open), `/tmp/audit/SUITE_RULES.md` § 6 (safety, renderer, regression and ECS audits, read three times), the coordinator's two dedup notes (ECS is canonical for the lock-order cycles), and closed-issue searches for "CloudSimState", "ActorVitals lock", "PendingScriptStoryEvents", "render_one_frame size pin" and "CONC-2026-10-21".

## Guard runs at HEAD

| Command | Result |
|---|---|
| `BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux --bin byroredux` | **RED: 3340 passed, 11 failed, 77 ignored.** 10 lock-order panics from 2 roots (ECS-2026-10-27-02 and -03). The 11th failure is `per_frame_driver_functions_stay_within_their_size_pins` (`render_one_frame` 728 lines vs pin 716), a Dup of REG-2026-10-27-01. |
| `BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux-core -p byroredux-physics -p byroredux-scripting --lib` | core 879/0, physics 270/0. **Scripting 1278 passed, 2 failed**: `obscript_anim::tests::*`, root ECS-2026-10-27-01. |
| CI run 40601712406 (HEAD), job "ABBA lock-order detector" | failure. Cycles grouped from the job log: 7× `ActorVitals → RapierHandles → GlobalTransform → ActorVitals`, 3× `CloudSimState → SkyParamsRes → CloudSimState`, 2× `AnimationPlayer → PendingGroupSwap → AnimationPlayer`. That is 12 panics from 3 roots, with no fourth cycle. |
| `cargo test -p byroredux-renderer --lib` | 1501 passed, 0 failed, 1 ignored |
| Scheduler proof (`build_scheduler_reports_zero_access_conflicts`, `scheduler_access_invariants_hold_on_the_real_schedule`, all 9 `system_access_declaration_tests`, ordering pins, CI-lane pins) | all `ok`; no `#[ignore]` in `scheduler_access_tests.rs` or `boot/schedule/mod.rs` |
| GPU-sync guards (`frames_in_flight_contract_names_every_dependent_resource`, `the_all_slots_wait_argument_is_pinned`, `render_finished_is_sized_and_indexed_per_swapchain_image`, `queue_guard_released_before_one_time_fence_wait`, `wait_failure_arm_disposes_only_on_device_loss`, `every_compute_consumer_of_the_skinned_vertex_ssbo_is_in_the_publish_dst_mask`, `taa_resolves_the_post_bloom_scene_tap`, `skip_clear_latches_are_promoted_after_submit_not_at_record_time`, `the_groundcover_field_flip_is_promoted_after_submit_not_at_record_time`, `a_frame_without_a_recorded_dispatch_does_not_advance_the_field`, `every_skipped_frame_drops_the_temporal_history_not_just_the_first`, both `dynamic_rgba` tests, the three #6146 / #6343 / #6348 layout-walk scans) | all `ok` |
| Physics and unload guards (`forcegreet_walk_with_real_physics_world`, `eat_walk_with_real_physics_world`, `unload_captures_do_not_invert_production_lock_orders`, `picked_up_restore_walks_meshes_before_taking_the_marker`, `view_restamp_does_not_close_the_render_skip_lock_cycle`) | all `ok` |
| Prior findings CONC-2026-10-21-01/02/03 | #6233, #6290 and #6291 are all closed and their guards pass |

### Lane state: the three cycles are Dups, but none is a reachable deadlock

The ABBA lane is red on three roots. The ECS audit (canonical) and the regression audit filed them, so they are not re-filed here. Reachability for each:

- **CloudSimState ↔ SkyParamsRes** is Dup of AUDIT_ECS_2026-10-27 ECS-2026-10-27-03 (also REG-2026-10-27-02).
  - The pair: `weather.rs:1264-1286` takes `SkyParamsRes` under the `CloudSimState` write guard (added in 4f2596bf6). `render/sky.rs:129-133` holds `SkyParamsRes` into `outdoor_sky_params`, which then takes `CloudSimState` at `:165`.
  - `weather_system` is an Early **exclusive**, and `build_sky_params` runs on the main thread after the `rayon::join`s (`render/mod.rs:1423`), so the two never overlap.
  - Fix: read the four tile scales before taking `CloudSimState`.
- **`vats_scan` GlobalTransform → ActorVitals** is Dup of AUDIT_ECS_2026-10-27 ECS-2026-10-27-02 (also REG-2026-10-27-03).
  - The `GlobalTransform` query guard lives across the **whole** target loop (`vats.rs:339-430`), not just `:346`. During the loop it:
    - takes `FormIdComponent`, `Dead`, `CreatureActor`, `FormIdPool`, `StringPool` and `SubtreeCache`;
    - re-reads `GlobalTransform` recursively in `bone_position` (`:436-444`);
    - enters `visible_fraction` (CONC-2026-10-27-01 below).
  - `vats_mode_system` is an Early exclusive, so there is no cross-thread deadlock today.
  - The fix must snapshot `(entity, translation)` and drop the guard. Moving the single `get` is not enough.
- **AnimationPlayer ↔ PendingGroupSwap** is Dup of AUDIT_ECS_2026-10-27 ECS-2026-10-27-01 (also REG-2026-10-27-04).
  - `apply_pending_group_swaps` (`crates/scripting/src/obscript_anim.rs:186-210`) records **both** orders inside one function: `sequence_player` re-takes `AnimationPlayer` (a recursive read) and `NamedSequences` under `PendingGroupSwap`.
  - The block's own comment says it was scoped to avoid exactly this.
  - It is an exclusive Update step, so the failure is self-contained.

None of the three is a live cross-thread deadlock. No other thread touches `World` while exclusives or the render assembly run, so no writer can be queued behind the recursive reads. They are still HIGH under the skill's red-lane rule, as the ECS and regression audits rated them.

The non-lock red tests are not concurrency findings: REG-2026-10-27-01 (size pin) and SAFE-2026-10-27-01 (lavapipe).

## Findings

This run has no NEW findings. Both rows below duplicate ECS-audit findings and are kept for nuance only. Counts: 0C / 0H / 0M / 0L new; 2 Dup rows (ECS-2026-10-27-02, ECS-2026-10-27-05).

### CONC-2026-10-27-01: vats `visible_fraction` takes `RapierHandles` + `ActorColliderOwner` under the `PhysicsWorld` guard
- **Severity**: MEDIUM as a standalone edge (precedent: #5955, the same shape in `projectile_flight_system`, was filed MEDIUM). It is folded into HIGH ECS-2026-10-27-02 and not counted separately.
- **Dimension**: RwLock Patterns (Resource↔Storage, Physics). It is reached from the ECS Lock Ordering scan.
- **Location**: `byroredux/src/systems/vats.rs:450-491` (`visible_fraction`). The inverse order is at `crates/physics/src/sync.rs:55-68` (`set_linear_velocity`: `RapierHandles` → `PhysicsWorld`).
- **Status**: Dup of AUDIT_ECS_2026-10-27 ECS-2026-10-27-02. That finding lists `PhysicsWorld` / `RapierHandles` / `ActorColliderOwner` among the locks taken under the `GlobalTransform` guard, and its fix builds `target_bodies` before the loop. It is kept here for two points ECS-02 does not make (below). Introduced in 7eb4de261 (2026-10-25, Refs #6350). Open #6429 covers the system's access declarations, not lock order.
- **Additional nuance**: the `PhysicsWorld → RapierHandles` edge is a PhysicsWorld-sink inversion against `set_linear_velocity` in its own right, independent of the `GlobalTransform` guard. A fix that only snapshots `GlobalTransform`, and leaves `target_bodies` built inside `visible_fraction` under `physics`, keeps it. The lane cannot confirm the fix either way (fixture blind spot, below).
- **Description**: `let Some(physics) = world.try_resource::<PhysicsWorld>()` is bound first. Then, while `physics` is alive, `world.query::<RapierHandles>()` is taken and `world.get::<ActorColliderOwner>(owner)` is called once per handle to build `target_bodies`. This records `PhysicsWorld → RapierHandles` and `PhysicsWorld → ActorColliderOwner`.
  - `docs/engine/ecs.md:716-732` requires "`PhysicsWorld` last, with nothing taken under it".
  - `set_linear_velocity` records `RapierHandles → PhysicsWorld`, so this is a true ABBA pair.
- **Evidence**:
  ```rust
  let Some(physics) = world.try_resource::<byroredux_physics::PhysicsWorld>() else { return 1.0; };
  let target_bodies: HashSet<_> = world
      .query::<byroredux_physics::RapierHandles>()
      .map(|handles| handles.iter().filter(|(owner, _)| world
          .get::<byroredux_physics::ActorColliderOwner>(*owner) ...
  ```
- **Trigger Conditions**: VATS entry or `vats.scan` with a live `PhysicsWorld`, a target actor in range and an equipped weapon. The call runs from an Early exclusive on the main thread, so it cannot deadlock today. A future parallel caller, or a lane fixture that inserts `PhysicsWorld`, turns it into a cycle.
- **Fixture blind spot**: the vats test world (`vats.rs` ~:676-700) registers no `PhysicsWorld`, so `visible_fraction` returns at `:457`. The detector has never recorded this edge, and the lane will stay silent on it after REG-03 is fixed.
- **Impact**: ECS lock-order discipline breach in new code (the #5371 / #5955 class), invisible to CI.
- **Related**: ECS-2026-10-27-02, REG-2026-10-27-03, #5955, #5371, #6429.
- **Suggested Fix**: build `target_bodies` before taking `PhysicsWorld`. It depends only on `target`, so compute it once per target outside the part loop. Then add a `PhysicsWorld` to one vats test so the lane sees the edge.

### CONC-2026-10-27-02: the #5647 collision arm's `PendingScriptStoryEvents` write is invisible to the access analyzer on `player_controller_system`
- **Severity**: LOW
- **Dimension**: Scheduler Access Declarations
- **Location**:
  - `byroredux/src/boot/schedule/early.rs:22-90`: the controller row declares `StoryEvent`, `OnDeathEvent` and `KillRecords`, not `PendingScriptStoryEvents`.
  - `byroredux/src/boot/schedule/mod.rs:760-781`: `PARALLEL_SYSTEMS` lists `raise_kill_story_event` and `note_death` but not `raise_story_event`.
  - `crates/scripting/src/story_manager.rs:150-170`: the collision arm calls `try_resource_mut::<PendingScriptStoryEvents>()`.
- **Status**: Dup of AUDIT_ECS_2026-10-27 ECS-2026-10-27-05. Kept for the hop-table half of the fix: `raise_story_event` is also missing from `PARALLEL_SYSTEMS`. This answers the skill's open question ("confirm whether the analyzer can see the write"): it cannot.
- **Description**: A drowning death in the parallel `player_controller_system` calls `raise_kill_story_event` (`systems/story_events.rs:258`), which calls `raise_story_event`. If the player already carries an undrained `StoryEvent` marker (for example a same-frame CLOC), the collision arm writes the `PendingScriptStoryEvents` resource. That write is neither declared on the row nor reachable by the mechanical scan, because `raise_story_event` is a cross-crate hop missing from `PARALLEL_SYSTEMS`.
- **Trigger Conditions**: none live. The only other Early-parallel system is `timer_tick_system`, which does not touch the queue. The `known_conflict_count() == 0` proof becomes unsound the moment another Early-parallel system reads or writes `PendingScriptStoryEvents`.
- **Impact**: a latent proof gap of the #5641 / #6217 class.
- **Related**: #5647, #5641, #6217.
- **Suggested Fix**: add `.writes_resource::<PendingScriptStoryEvents>()` to the controller row and `(STORY_MANAGER_SRC, "raise_story_event")` to its `PARALLEL_SYSTEMS` hops.

## Per-dimension notes

### Dim 1: Vulkan queue and AS sync
`git diff 4a3e89d56..HEAD` adds no `queue_submit`, `queue_present`, `wait_for_fences`, `device_wait_idle` or `destroy_acceleration_structure` call. `draw.rs:588-596` and `:718-727` still bind the queue `MutexGuard` across the submit and the present after the #6143 split. The #6250 `moved_surface_buffers` are per-FIF (`buffers.rs:575`, `upload.rs:862`) and ride the all-slots wait. Their missing destroy is Dup of SAFE-2026-10-27-01. No findings.

### Dim 2: compute chains
- #6249 adds a compute read of the G-buffer mesh-ID (shadow filter binding 5). The main render pass's outgoing dependency (`context/helpers.rs:346-385`, dst `FRAGMENT_SHADER | COMPUTE_SHADER` / `SHADER_READ`) already publishes it, on the same edge SVGF uses.
- Ground-cover wind (1ca5032ae) only grows `ModelPush` (64 → 96 B push constants).
- The underwater god rays add no descriptor.
- The moved-surface cap overflow is REN-D2-01.

No findings. This dimension was read only; there was no `BYRO_VALIDATION` run.

### Dim 3: ECS lock ordering
See the lane table and CONC-2026-10-27-01. The new system bodies hold:
- `find_package`: the ce22fcbcf candidate snapshot.
- `flee_package`: `PhysicsWorld` is held only over the pure step loop.
- `faction_hostility`: `RapierHandles` is snapshotted before a per-cast guard.
- `trap_hits`: overlaps are collected and the guard dropped before actor reads.
- `combat` melee sweep.
- `crime.rs`: expression-scoped guards.
- Ragdoll #6134 / #6326 park and restore.

`extensions/` has had no commits since 2026-10-21.

### Dim 4: scheduler proof
Live and non-vacuous. VATS, Flee / UseWeapon / Ambush / Find, crime and jail all register as exclusives. `vats_mode_system`'s under-declaration is open #6429 and is not a proof issue, because the system is exclusive. See CONC-2026-10-27-02.

### Dim 5: RwLock patterns and physics
The physics lane is green (270/0). In the PhysicsWorld-sink re-scan over every site changed since 2026-10-21, `vats.rs::visible_fraction` (CONC-2026-10-27-01) is the only violation.

### Dim 6: resource lifecycle
The #6343 / #6146 / #6348 guards pass. The only defect is Dup of SAFE-2026-10-27-01 / REN-D5-01.

### Dim 7: worker threads
- The new threads are `crates/audio/src/fuz.rs:359/364` (#6307 `run_bounded` pipe drains). Both are joined after the child exits or is killed; stdout is bounded by `take()` and the kill comes at the 10 s `FFMPEG_TIMEOUT`.
- `weather_settings.rs:512` and `animation_graph.rs:2312` are test-only.
- `dialogue_voice.rs:282` still decodes `.fuz` on the main thread. It is now bounded at 10 s; that stall cost belongs to the audio and performance audits and is not filed here.
- #6407 changed only the `SharedParseCache` gate predicate, not its lock scope.
- `render/mod.rs`'s joins only gained `PlayerEntity::get` reads.

No findings.

Next step: nothing to publish from this report. Both rows are Dups of ECS-2026-10-27-02 / -05. When those are published, add the nuance above to their issue bodies: the sink inversion plus the fixture-blind test, and the `raise_story_event` hop.
