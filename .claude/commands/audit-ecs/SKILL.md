---
description: "Deep audit of the ECS — storage backends, queries, world, systems, resources"
---

# ECS Audit

Read `_audit-common.md` and `_audit-severity.md` for shared protocol.

Scope: `crates/core/src/ecs/` (one file per concern: `storage.rs` traits, `packed.rs` / `sparse_set.rs`
backends, `world.rs`, `query.rs`, `resource.rs`, `scheduler.rs`, `access.rs`, `lock_tracker.rs`,
`systems.rs` transform propagation, `hierarchy.rs`), its `components/` + `resources/` trees, the
`crates/core/src/animation/` runtime (Dim 9), and scheduler *registration* in `byroredux/src/boot/schedule/`.
Owner map: _audit-owners.md. Gameplay-system logic (combat, inventory, AI-package behaviors,
containers/loot, notifications) belongs to `/audit-gameplay`; this audit keeps only ECS *shape* — storage
class, lifecycle, lock/access declaration.

Dimensions run in order of blast radius. Per-dimension `Paths:` / `First step:` let you skip a
dimension whose Paths have no commits since the last report
(`git log --since=<last-report-date> --format='%h %s' -- <Paths>`).

## Dimensions

### 1. Lock Ordering & Deadlock Machinery (HIGHEST blast radius)

Paths: `crates/core/src/ecs/{world,lock_tracker,query,resource}.rs`
First step: `cargo test -p byroredux-core lock_tracker` then `BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux-core`

A wrong lock order is HIGH. This dimension owns the *machinery*; system-level cycles, canonical-order
adherence and CI detector coverage are `/audit-concurrency` Dim 3 and Dim 5.

- **Reentrancy**: the thread-local check (write on a type already held, read while holding write)
  panics in debug AND release. The global ABBA graph is debug-only and opt-in via
  `BYRO_LOCK_ORDER_CHECK=1` (#313). Every `query` / `query_mut` / `resource` / `resource_mut` site in
  `world.rs` must arm a `TrackedRead` / `TrackedWrite` scope, defuse it only AFTER the real lock is
  taken, and untrack in `Drop`.
- **TypeId-sorted pairs**: `query_2_mut`, `query_2_mut_mut`, `resource_2_mut`, `try_resource_2_mut`
  acquire in `TypeId` order AND arm tracker scopes in the same order (arming in parameter order
  re-opens #313 — a caller spelling `<B, A>` then looks like ABBA). Each `assert_ne!`s `A == B`; a
  silent self-deadlock is the regression. `try_resource_2_mut` checks BOTH existences before taking
  EITHER lock (#465).
- **Tracker internals** (each pinned by tests in `lock_tracker.rs` — read them, do not re-derive):
  `record_and_check` runs BEFORE the incoming `LockState` row is inserted, on the fresh-acquire AND
  recursive-read paths (#2384/#3696; `is_clean()` true after a caught ABBA panic); `GRAPH` lock
  poison is recovered with `unwrap_or_else(|poison| poison.into_inner())`, never `.expect` (#2385 —
  an `.expect` blinds the detector after its first correct catch); a recursive read is a deduplicated
  `log::warn!`, not a reject (#2386/#3249: `recursive_read_warns_once_and_continues`); the `held_others`
  snapshot is gated on `cfg(debug_assertions)` AND `global_order::is_enabled()` (#823/#3680) so a
  detector-off debug build allocates nothing.
- **Poison**: every acquisition resolves `PoisonError` through `storage_lock_poisoned` /
  `storage_lock_poisoned_erased` / `resource_lock_poisoned` (`world.rs`) — re-panic loud with the type
  name. `despawn` uses the erased variant fed by the `type_names` side-table (#466); dropping either
  loses the type name in every panic.

### 2. Storage Correctness & Change Tracking

Paths: `crates/core/src/ecs/{packed,sparse_set,storage}.rs`, `crates/core/src/ecs/components/{transform,global_transform,hierarchy,scene_flags,world_bound}.rs`
First step: `grep -rn 'type Storage = PackedStorage' crates byroredux --include='*.rs' | grep -v test`

- **PackedStorage census**: production users are exactly `Transform`, `GlobalTransform`, `WorldBound`,
  `SceneFlags`. Any other `PackedStorage` component is a finding unless it is read every frame by
  renderer/physics/animation. Everything sparse (actors, markers, events) is `SparseSetStorage`.
- **SparseSetStorage**: swap-remove repoints the sparse slot of the entity moved into the gap;
  removing the last element takes the no-swap path; re-insert overwrites in place (`swap_remove`,
  `remove_last`, `overwrite`).
- **PackedStorage**: `binary_search` keeps the sorted-by-entity invariant on insert/remove;
  `insert_bulk` is append + one sort and the result stays sorted AND deduplicated (#467);
  `World::insert_batch` still fires the per-item `entity < next_entity` `debug_assert`.
- **Change tracking** (`Component::TRACK_CHANGES`, default `false`): ON for SEVEN
  components — `Transform`, `GlobalTransform`, `Parent`, `Children`, plus `LocalBound`
  (since `ad012f9d6`) and `Material` and `ParticleEmitter` (both since `1d56758ba` /
  #3836). The last three use sparse storage, so they get only a
  `structural_generation` bump and no dirty set — #3836's `SceneEffectSoftCache`
  and the incremental world-bound propagation consume them.
  `PackedStorage` keeps a dirty set (may hold duplicates —
  consumers tolerate that); `SparseSetStorage::structural_generation` bumps on insert/remove.
  `drain_dirty_into` preserves capacity (#1371); `take_dirty` hands it away. The `GlobalTransform`
  dirty set has ONE destructive drainer (`make_world_bound_propagation_system`,
  `byroredux/src/systems/bounds.rs`) — a second `take_dirty` / `drain_dirty_into` consumer steals its
  work; other systems must use `get_mut` only where marking dirty is intended (billboards call this
  out). A mutation path that forgets `mark_dirty` silently breaks Dim 6's fast path.
- **Erased removal (per cell unload)**: `clear_erased` releases capacity; `remove_entities_erased`
  early-outs on an empty victim set and, in `PackedStorage`, compacts in place with zero allocations
  (`remove_entities_erased_does_not_reallocate`, #3689) while keeping sort order + dirty marks (#2396).
  A regression is a per-streaming-cycle cost.
- `EntityId` is monotonic and never recycled (`World::spawn` `checked_add` panic #36; `despawn`
  reclaims nothing, #372/#3375). Do not describe it as generational or "fix" it by reuse (dangling
  `Parent` refs go silent).

### 3. Query Borrow Safety & the ECS `unsafe`

Paths: `crates/core/src/ecs/query.rs`, `.github/workflows/ci.yml` (job `ecs-query-miri`)
First step: `grep -n 'unsafe' crates/core/src/ecs/query.rs` (expect exactly 4 derefs)

- The only `unsafe` in `ecs/` is the four cached-pointer derefs in `query.rs` (`QueryRead::storage`,
  `QueryWrite::storage` / `storage_mut`, `ComponentRef::Deref`; #1367). Each needs a SAFETY comment
  tying the pointer to the live guard field; the `#[allow(dead_code)] guard` must not drop early;
  `&mut *self.storage` stays gated by `&mut self`. A new unsafe block without a comment is MEDIUM.
- `World::get` returns `ComponentRef` (owns its guard), never a raw pointer to dropped storage (the
  unsound #35 pattern = CRITICAL UAF if it returns).
- Guard: CI job `ecs-query-miri` runs `cargo miri test -p byroredux-core --lib ecs::world::tests`
  skipping only `resource_visible_to_system_via_scheduler` (crossbeam-epoch TLS vs Miri). Confirm the
  skip list has not widened and that any new cached-pointer site has a test under `ecs::world::tests`.
- `query` / `query_mut` return `None` for never-created storage (no lazy creation); `register::<T>()`
  guarantees success before first insert.
- `HierarchyTraversalGuard` (`crates/core/src/ecs/hierarchy.rs`) bounds every parent/children walk to
  `entities + child refs + 1` steps; transform propagation and bounds use it. A new hierarchy walk
  without it, or without a visited set, is an unbounded-loop hazard on a cyclic `Parent` graph.

### 4. Resources & World-Level State

Paths: `crates/core/src/ecs/{resource.rs,resources/,game_profiles.rs,debug_load.rs,metrics.rs}`, `world.rs` resource half
First step: `git log --since=<last-report-date> --format='%h %s' -- crates/core/src/ecs/resources`

- `resource()` / `resource_mut()` panic with the type name when never inserted; `try_*` return `None`.
  `ResourceRead` / `ResourceWrite` downcast per `Deref` (not cached — not the #1367 hot path);
  `insert_resource` returns the prior value; `remove_resource` resolves poison.
- **Resource vs. component choice**: mode flags and singleton state are `Resource`s (`HardcoreMode`,
  `SelectedRef`, the `*Bridge`/`*Telemetry` structs); per-actor state is a `SparseSetStorage`
  component (`ActorVitals`, `TimedRestorations`). A new per-entity fact modelled as a global map keyed
  by `EntityId` is the drift (it defeats `despawn` cleanup and save capture). Every new `Resource` /
  `Component` is also subject to the save registry gate
  (`every_component_or_resource_impl_is_saved_or_explicitly_allowlisted`, `byroredux/src/save_io/`,
  `/audit-save`) and the debug-server registry.
- `SkinSlotPool` (`resources/skin_slot_pool.rs`): every collection is `FxHashMap`/`FxHashSet`
  (`_audit-common.md` hot-path hashing rule); `pose_dirty` and `drain_pending` are the renderer
  hand-off. Overflow/retry semantics are `/audit-safety` Dim 7.
- `OwnershipTracker` / `ReclaimPolicy` (`resources/ownership.rs`): the EX-08 soak-gate accounting.
  Check each owner class's policy is honest — `Monotonic` only for identity watermarks (entity ids,
  mesh/texture slot-vector lengths), `Exact` for anything that must return to baseline after unload,
  `Bounded` for documented caches. A leaking class reclassified `Bounded`/`Monotonic` to turn the gate
  green is the finding.
- `DeltaTime` is stamped once per frame by the main loop (`byroredux/src/app_events.rs`); a system that
  writes it or `TotalTime` is a finding.

### 5. System & Scheduler Wiring, Declared Access

Paths: `crates/core/src/ecs/{scheduler,access,system}.rs`, `byroredux/src/boot/schedule/`, `byroredux/src/boot/registries.rs`, `byroredux/src/scheduler_access_tests.rs`
First step: `cargo test -p byroredux -- system_access_declaration_tests scheduler_access` then `rg -n '^\s*scheduler\.add_to\(' byroredux/src/boot/schedule/` (must be empty)

Stages: `Early → Update → PostUpdate → Physics → Late` (`Stage`, discriminants 0..=4, ordered by
`BTreeMap`); "exclusive" is a *phase inside every stage* (`StageData.exclusive`, serial after that
stage's parallel batch), not a stage. Registered per stage in `register_{early,update,post_update,physics,late}_systems`.

- **The `Access` model**: `Access::new().reads::<T>().writes::<U>().reads_resource::<R>()…`;
  declared / declared-empty / undeclared (`None`). `AccessConflict` has exactly `None`,
  `Unknown { left_undeclared, right_undeclared }`, `Conflict { pairs }` (no `Parallel` variant);
  `analyze_pair` returns `Unknown` when either side is undeclared. Closures/bare fns cannot override
  `System::access`, so they declare at registration via `add_to_with_access` /
  `add_exclusive_with_access`. Pins: `undeclared_closure_pairs_show_as_unknown`,
  `exclusive_systems_are_listed_but_not_paired`, `all_five_stages_run_in_order` (reordering or
  inserting a stage without updating it is the regression).
- **Guard (mechanical declaration completeness)**:
  `byroredux/src/boot/schedule/mod.rs::system_access_declaration_tests` —
  `every_parallel_system_declares_everything_it_acquires` scans each parallel system's body (same-file
  callees to depth 3, plus the explicit cross-file hops in `PARALLEL_SYSTEMS`) and fails if an acquired
  type is missing from its `Access`; `the_parallel_system_table_covers_every_parallel_registration`
  fails when an `add_to_with_access` lands outside the table; two exclusive fns are covered too
  (`papyrus_provider_system`, `legacy_obscript_load_order_system`).
  Sibling gates in `scheduler_access_tests.rs`: `scheduler_access_invariants_hold_on_the_real_schedule`
  (non-vacuous floors: ≥9 parallel systems, ≥7 analysed pairs, then 0 undeclared / 0 conflicts / 0
  unknown), `contract_bearing_exclusives_declare_their_access`, `p2_gameplay_exclusives_declare_non_empty_access`,
  `late_telemetry_declarations_read_all_their_resources`. Boot enforces the same three counts as
  RELEASE `assert_eq!`s in `install_runtime_registries` (#1394/#1602/#2690) — not `debug_assert!`, not a
  log line. Confirm the guards are live: `rg -n '#\[ignore' byroredux/src/scheduler_access_tests.rs byroredux/src/boot/schedule/mod.rs` returns nothing, and `PARALLEL_SYSTEMS.len()` equals the
  `add_to_with_access(` count.
  **What the guard cannot see** (audit these by hand for every parallel system and any exclusive being
  promoted to parallel): it matches only turbofish `query::<T>` / `query_mut` / `resource` /
  `resource_mut` (+ `try_`) — NOT `world.get::<T>` / `get_mut` / `has::<T>`, `query_2_mut::<A, B>`,
  `resource_2_mut`, or types inferred without a turbofish; it does not follow hops into a different file
  unless listed in the table; closures and macro bodies are opaque.
  (#4573 closed the mode/substring/comment blind spots: read-vs-write IS compared, names match
  whole declared types, and `//` lines in the registration block no longer satisfy the scan. The
  forms gap above remains.) A same-session precedent: an
  undeclared same-frame `GlobalTransform` write added to `fly_camera_system` made the boot
  `known_conflict_count() == 0` proof unsound until the registration declared it (commit ac1d44f5c).
- **Exclusive declarations are optional**: most `add_exclusive` registrations are undeclared by design
  (`undeclared_exclusive_count()` non-zero); the analyzer never pairs exclusives. Flag only a *parallel*
  system that lost its declaration, or an exclusive that a test above says must declare. Enumerate live
  counts with `rg -c` on the four `add_*` forms; never quote a total.
- **Panic policy is fail-fast** (#1412): do not report a missing `catch_unwind`. `Scheduler::run` takes
  `&mut self` and `Scheduler` is deliberately not a `Resource` (re-entry impossible, #868).
  `add_*` warns on duplicate names; `try_add_*` returns `Err(name)` across the flat name space (#312).
- Cross-stage sequencing (a consumer in an earlier stage than its producer) is invisible to the
  analyzer — see `/audit-concurrency` Dim 4 for the pinned cases and the by-hand rule.

### 6. Hot-Path Performance Invariants (regression guards)

Paths: `crates/core/src/ecs/systems.rs`, `byroredux/src/systems/{animation,audio,bounds}.rs`, `byroredux/src/components.rs`
First step: `cargo test -p byroredux-core ecs::systems`

- **Transform propagation fast path** (#825/#1371, `make_transform_propagation_system`): the cached
  `roots` set is keyed on `(Transform::len(), Parent len-or-0, next_entity_id())` plus the `Parent` /
  `Children` `structural_generation()` values and the drained `Transform` dirty set; the BFS is skipped
  when nothing changed. Uses `drain_dirty_into`, not `take_dirty`. A path that stops bumping
  `structural_generation` / `mark_dirty` yields a wrong `GlobalTransform` — a correctness bug, escalate.
- **PostUpdate/Late ordering contract** (comments in `boot/schedule/post_update.rs`): transform
  propagation, then bound propagation (drains the `GlobalTransform` dirty set), and no `Stage::Late`
  system may write `GlobalTransform` on a `LocalBound`-bearing entity (its `WorldBound` lags a frame;
  billboards are the one accepted exception). A new Late `GlobalTransform` writer must make that call.
- **Animation scratch** (`byroredux/src/systems/animation.rs`): the `NameIndex.map` refill is in place
  (`clear` + reserve + reinsert; a fresh map costs a ~3 ms stream-in spike, #824); `SubtreeCache` clears
  only when the `Name` count changes (#278); `events` / `seen_labels` scratch is hoisted and
  `clone`d not `mem::take`n (#828). Lock order inside channel apply is content-determined
  (#2399) — do not reintroduce a macro that hides the acquisition order.
- **`FootstepScratch`** (`byroredux/src/components.rs`): `mem::take` + restore so `Vec` capacity
  survives; a per-frame `Vec::new` regresses (#932). Placement (Late exclusive after
  `camera_follow_system`) is pinned by `footstep_runs_after_camera_follow_in_late`.

### 7. Component Lifecycles (load/unload, transient, idempotency)

Paths: `byroredux/src/{streaming,npc_spawn}*`, `byroredux/src/cell_loader/unload.rs`, `crates/core/src/ecs/components/`, `crates/scripting/src/{events,timer,cleanup}.rs`, `crates/core/src/animation/registry.rs`
First step: `cargo test -p byroredux rapier_release` then read `git log --since=<last-report-date> -- crates/core/src/ecs/components`

- **Cell load/unload symmetry** (`streaming.rs`, `cell_loader/unload.rs`): every component/resource row a
  cell load attaches is removed on unload (no orphan `CharacterController`, `LightFlicker`,
  `RapierHandles`, animation players, `SeatReservations` claims whose furniture or claimant is gone).
  Spawn dispatch is idempotent — one REFR FormId never spawns twice.
- **Behavior components stay sparse**: every AI-procedure `*Behavior` / `*State` / terminal marker is
  `SparseSetStorage`. The roster is pinned in the debug registry by
  `roster_tests::every_ai_procedure_behavior_component_is_registered`
  (`crates/debug-server/src/registration.rs`, #4063); teardown completeness
  (`clear_ambient_behavior`, `npc_spawn/ai_package.rs`) and behavior semantics are `/audit-gameplay`.
- **Transient markers**: `ActivateEvent` / `HitEvent` / `TimerExpired` are removed by
  `event_cleanup_system` (Late exclusive, registered after every consumer; a second cleanup site would
  double-free). `timer_tick_system` never accumulates negative time.
- **`AnimationClipRegistry`** (`animation/registry.rs`): interns by ASCII-lowercased path (#790) so
  streaming does not grow it; `release()` clears a slot but never returns it — no free list, by design
  (#2689), because a released handle may still sit on an `AnimationPlayer` / `AnimationLayer`. Every
  evict/reload strands one empty stub (`stub_slot_count()`, visible not closed). Do not propose a
  free list without addressing that aliasing hazard.
- **No animation controller layer**: `AnimationStack` is the whole sequencing surface; audit its
  lifecycle (no dangling clip refs after unload) rather than looking for a controller above it.
- **Emitters**: `apply_emitter_params` (`byroredux/src/systems/particle.rs`) fills `ParticleEmitter`
  from `ImportedEmitterParams`; size is `initial_radius × base_scale.unwrap_or(1.0)` and colour is not
  clobbered (`apply_emitter_params_size_defaults_base_scale_to_one`,
  `apply_emitter_params_overrides_kinematics_and_size_not_color`). Mapping correctness is `/audit-nifal`.
- **Physics/audio handle lifetimes**: Rapier release on unload is `/audit-safety` Dim 3; kira
  spatial sub-track handles drop before the listener; `OneShotSound` markers are pruned once kira
  reports `Stopped` (`/audit-audio`).

### 8. NIFAL Canonical Material in the Component Layer

Paths: `crates/core/src/ecs/components/material.rs`, `byroredux/src/material_translate.rs`
First step: `cargo test -p byroredux-core resolve_pbr`

`Material` is the landing zone of the NIFAL boundary (upstream is `/audit-nifal`).

- **Plain-`f32` contract**: `metalness` / `roughness` are resolved `f32`, not `Option<f32>` (a regression
  re-introduces per-draw classification — HIGH).
- **Single mutation site**: `material_translate.rs::translate_material` is the sole `ImportedMesh →
  Material` boundary; `Material::resolve_pbr` is the only fill-the-gap helper (shared
  `classify_pbr_keyword`, fills only the unset slot). No per-draw `classify_pbr` fallback survives in
  `byroredux/src/render/static_meshes.rs`.
- Pinned by `resolve_pbr_is_idempotent`, `resolve_pbr_preserves_upstream_translator_values`,
  `resolve_pbr_fills_only_missing_slot`, `resolve_pbr_clamps_authored_out_of_range`; clobbering authored
  scalars or breaking idempotency is a finding.
- Other producers (Starfield CDB via `crates/sfmaterial/`) go through `translate_material` /
  `resolve_pbr`; `crates/debug-ui/` must not register or mutate gameplay components.

### 9. Animation Runtime (`crates/core/src/animation/`)

Paths: `crates/core/src/animation/`, `byroredux/src/systems/animation.rs`, `byroredux/src/anim_convert.rs`
First step: `cargo test -p byroredux-core --features inspect animation`

Import is `/audit-nif`, the NIF→clip boundary is `/audit-nifal` Dim 7; this dimension owns sampling,
blending, root-motion split and text-key dispatch, driven by an ECS system.

- **Clip-handle validity**: `AnimationPlayer.clip_handle` / `AnimationLayer` index the registry; a stale
  handle after unload is a no-op, never a panic or OOB read.
- **Time advance**: `advance_time` (`player.rs`) / `advance_stack` (`stack.rs`) handle `dt == 0`,
  negative/NaN `dt` and a zero-length clip without divide-by-zero or an endless loop; `CycleType` is
  applied per clip.
- **Blend weights**: `AnimationLayer::effective_weight`, `play`, `cleanup_finished`, with the live ramp
  written by `advance_stack` (`weight = blend_in_target * progress` every tick, #3701). Blend timers
  tick ahead of the `playing` gate so a paused blend-out layer still retires (#3702,
  `advance_stack_ticks_blend_out_on_a_paused_layer_so_it_can_be_retired`). Check that
  `cleanup_finished` never removes a layer still contributing weight and that `play` per tick with a
  nonzero blend time cannot grow the stack unboundedly.
- **`sample_blended_transform`** is the per-bone-per-frame hot path: no allocation, single-layer
  short-circuit bit-identical to the general path (#3706,
  `single_layer_short_circuit_matches_two_pass_output`). Cost belongs to `/audit-performance`.
- **Root motion**: `split_root_motion` applies once per tick and is drained — an undrained
  `RootMotionDelta` integrates every frame.
- **Text keys**: `visit_stack_text_events` (`stack.rs`) emits each key exactly once across a loop
  wrap and still emits when one large `dt` spans several key times.
- **Interpolation** (`interpolation.rs`): `find_key_pair` at t < first / t > last key; quaternion
  shortest-path (missing dot-sign flip = bone spinning the long way). B-splines occur on FNV/FO3 too.

## Process

1. Scope: run each dimension's `First step:`; skim dimensions whose Paths are unchanged.
2. `cargo test -p byroredux-core --features inspect` and `cargo test -p byroredux` (counts live in
   ROADMAP.md; do not pin a number).
3. Save the report to `docs/audits/AUDIT_ECS_<TODAY>.md`.
