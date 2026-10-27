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
AND `BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux-scripting` AND `BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux --bin byroredux`
(rustc ≥ 1.96 toolchain — wasmtime 49's MSRV since 15a6b1d2d) — or read the last main run's `ABBA lock-order detector`
job conclusion via `gh run view <id> --json jobs`. **Never skip this step on a delta-scoped run**, even when
this dimension's Paths are unchanged: the process-wide graph is built by every *caller*, and the cycles live in
the binary's graph, not core's — core stayed green through the whole 2026-09-22..28 red streak (#4985, #4982).
A `&mut World` function still records edges the detector cannot tell apart from `&World` ones, so hoisting
several read guards into one block for speed (#4616) reddens the lane. The `-p byroredux` lane went red again
on three production inversions — a gather taking `GlobalTransform` before `Transform` (every mover takes
`Transform` first), a hair head-mount holding `MeshHandle` + `Children` across a `Parent` walk, a seated
restore holding `Furniture` across a `GlobalTransform` read — and was re-greened by #5535 `1cb12fb3d`; a new
multi-storage gather is the regression shape. It recurred within two days: #5713's `cfabdfd6a` called
`collect_subtree` (takes `Children`) while a `MeshHandle` guard was alive in `common_shape_rotation`, re-recording
the `MeshHandle → Children` edge #5535 removed; #5954 `7a4f6c1a2` gathers first, then filters. Test bodies feed the
same graph: #5032 `3bc60e0be` made `cinematic_trio_survives_save_load_round_trip` (`save_io/round_trip_tests.rs`)
and `quest_alias_collection_fill_type_still_declines_not_binds` (`crates/scripting/src/scene/quest_alias_tests.rs`)
drop their first guard before the second acquisition — a test that holds a `ComponentRef`/`ResourceRead` across
another acquisition records a back edge exactly like production code.

A wrong lock order is HIGH. This dimension owns the *machinery*; system-level cycles, canonical-order
adherence and CI detector coverage are `/audit-concurrency` Dim 3 and Dim 5.

- **Reentrancy**: the thread-local check (write on a type already held, read while holding write)
  panics in debug AND release. The global ABBA graph is debug-only and opt-in via
  `BYRO_LOCK_ORDER_CHECK=1` (#313). Every `query` / `query_mut` / `resource` / `resource_mut` site in
  `world.rs` must arm a `TrackedRead` / `TrackedWrite` scope, defuse it only AFTER the real lock is
  taken, and untrack in `Drop`.
- **TypeId-sorted pairs**: `query_2_mut`, `query_2_mut_mut`, `resource_2_mut`, `try_resource_2_mut`,
  and the read-only `try_resource_2` (#6101 `824c620a5`) acquire in `TypeId` order AND arm tracker scopes in
  the same order (arming in parameter order re-opens #313 — a caller spelling `<B, A>` then looks like ABBA).
  Each `assert_ne!`s `A == B`; a silent self-deadlock is the regression. `try_resource_2_mut` and
  `try_resource_2` check BOTH existences before taking EITHER lock (#465). Two nested single `try_resource`
  reads of a pair that some other site takes through a `*_2` helper are the #6101 shape: `TypeId`s move with the
  unified feature set, so the inversion reddens only the build configuration that hashes the pair the other way
  (it surfaced in the multi-crate `-p byroredux-core -p byroredux-physics -p byroredux-scripting --lib` lane);
  pinned by `try_resource_2_does_not_deadlock_against_resource_2_mut` (`crates/core/src/ecs/world_tests.rs`).
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
- **Change tracking** (`Component::TRACK_CHANGES`, default `false`): ON for SIXTEEN
  components (`rg -c 'const TRACK_CHANGES: bool = true' crates byroredux`, minus the one `packed.rs` test type) —
  `Transform`, `GlobalTransform`, `Parent`, `Children`, plus `LocalBound`
  (since ad012f9d6), `Material` and `ParticleEmitter` (both since 1d56758ba /
  #3836), `CollisionShape` (`crates/core/src/ecs/components/collision.rs`) + `RapierHandles`
  (`crates/physics`), both since 88c23887b, `RigidBodyData` (same file, #5943 `44f3a4f35`),
  `FormIdComponent`, `PhysicsSourceForm` and `PapyrusReferenceScripts` (#6312 `9290248d6`) and `AnimationPlayer`,
  `AnimationTarget` and `ArchetypeIdle` (#6334 `86ac93e0c`). The sparse ones get only a `structural_generation` bump and no dirty set — #3836's `SceneEffectSoftCache`,
  the incremental world-bound propagation, `physics_sync_system`'s unregistered-shape absence
  cache (`sync.rs`), `trap_contact_system`'s trap → scripted-owner map (`TrapOwnerCache`,
  `byroredux/src/systems/trap_hits.rs`, rebuilt only when the six storages it derives from change membership) and
  `archetype_idle_system`'s settled-pass gate (`byroredux/src/systems/archetype_idle.rs`) and
  `FormIdPool::entity_with_local` (`crates/core/src/form_id.rs`, #6338 `ab3a604bf`: the local → entity index the
  Flee / UseWeapon package systems resolve a target through per actor per frame, keyed on `FormIdComponent`'s
  generation + the pool length, behind a leaf `Mutex`; a duplicate id keeps the first entity in storage order,
  `entity_with_local_reuses_its_build_until_the_membership_moves`) consume them. The generation moves on insert (overwrite included) and
  remove, never on `get_mut` — a cache keyed on a field (the trap list on `havok_layer`) goes stale if that
  field is ever edited in place.
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
  `Parent` refs go silent). The "generation-keyed" index of #6338 keys on a *storage's* `structural_generation`,
  not on an entity generation.

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
  skipping only `resource_visible_to_system_via_scheduler` (crossbeam-epoch TLS vs Miri); the one other exception,
  the #6101 lock-order stress test, carries `#[cfg_attr(miri, ignore)]` in `world_tests.rs` instead of a `--skip`
  (#6234 `5dc642ee7`). Confirm the skip list and that `cfg_attr(miri, ignore)` set have not widened and that any new cached-pointer site has a test under `ecs::world::tests`.
- `query` / `query_mut` return `None` for never-created storage (no lazy creation); `register::<T>()`
  guarantees success before first insert.
- `HierarchyTraversalGuard` (`crates/core/src/ecs/hierarchy.rs`) bounds every parent/children walk to
  `entities + child refs + 1` steps; transform propagation and bounds use it. A new hierarchy walk
  without it, or without a visited set, is an unbounded-loop hazard on a cyclic `Parent` graph.
  The shared guarded collector `collect_subtree` (same file; visited set + `HierarchyTraversalGuard` budget,
  `collect_subtree_terminates_on_a_cycle`) landed with #5513 `9da80470d`: `retry_cinematic_readoption`,
  `loading_screen.rs`, `cinematic_retained_entities` / the unload purge (`cell_loader/unload.rs`),
  `release_finished_tethers` and `subtree_entities_under` all route through it, and #5713 `cfabdfd6a` moved the
  last five hand-rolled whole-subtree walks onto it (`player_owned_entities` in `cell_loader/unload.rs`,
  `player_body.rs`'s descendant walk, `subtree_has_skinned_mesh` + `common_shape_rotation` in
  `npc_spawn/resumable/runtime.rs`, the water-plane scan in `cell_loader/water.rs`). Nothing mechanical forces a
  new whole-subtree walk to use it — grep for hand-rolled `Children` stacks; the guarded walks still outside it
  (propagation in `systems.rs`, `systems/bounds.rs`, `ragdoll.rs`, the subtree name map in `anim_convert.rs`)
  carry their own visited set + budget. Call the collector *before* taking any other storage guard (#5954 above).

### 4. Resources & World-Level State

Paths: `crates/core/src/ecs/{resource.rs,resources/,game_profiles.rs,debug_load.rs,metrics.rs}`, `world.rs` resource half
First step: `git log --since=<last-report-date> --format='%h %s' -- crates/core/src/ecs/resources`

- `resource()` / `resource_mut()` panic with the type name when never inserted; `try_*` return `None`.
  `ResourceRead` / `ResourceWrite` downcast per `Deref` (not cached — not the #1367 hot path);
  `insert_resource` returns the prior value; `remove_resource` resolves poison; `init_resource::<R>()`
  inserts `R::default()` only when absent, leaving an existing (e.g. save-restored) value untouched (#5806
  `d0572ed94` — it replaced the hand-spelled `try_resource`/`insert_resource` idiom in scripting's `register`;
  #6136 `10360816e` moved it below `insert_resource`, which it had split from its doc block, and pinned both with
  `init_resource_inserts_the_default_only_when_absent` / `insert_resource_keeps_its_own_doc` in `world_tests.rs`;
  #6366 `f6c7c534e` converted the last five hand-spelled copies, and `no_source_spells_insert_default_if_absent_by_hand`
  in `byroredux/src/workspace_hygiene_tests.rs` scans the bin and every crate so a fourth spelling fails the build — a
  site inserting a *configured* value, such as `TerrainHeightService`, is legitimately different).
- **Read accessors, one spelling each**: the bin reads the player entity only through `PlayerEntity::get(world)`
  (`byroredux/src/systems/character.rs`, #6362 `67722b6e4` — it replaced 66 hand-spelled reads, and
  `no_source_hand_spells_the_player_entity_read` keeps it so). It is deliberately NOT synchronised with the scripting
  crate's `PapyrusPlayerEntity::get`, which differs in FlyCam, so a flow crossing the crate boundary reads the pair
  knowingly. Doc blocks spliced onto the wrong item (the shape that split `insert_resource`'s doc) are scanned
  workspace-wide by `no_doc_block_is_spliced_onto_the_wrong_item` (`core::source_scan::spliced_doc_blocks`, #6373
  `546f5e2d4`); it only catches a doc that leads with a sibling's name.
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
  `Bounded` for documented caches, `Occupancy` only for per-frame camera-pose samples (the two ground-cover
  classes; a strictly increasing series must also rise past `OCCUPANCY_GROWTH_TOLERANCE_PERMILLE` of its first
  sample, #6058 `adaa7a263`). A leaking class reclassified `Bounded`/`Monotonic`/`Occupancy` to turn the gate
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
  fails when an `add_to_with_access` lands outside the table; five exclusive fns are covered too
  (`npc_combat_ai_system`, `papyrus_provider_system`, `legacy_obscript_load_order_system`,
  `equipment_appearance_system` — its mid-life gear-import hop into `npc_spawn/loot_appearance.rs`, #5069
  `9355fcb5c` — and `npc_dialogue_selection_system` — #5307, plus its `play_line_voice` / `raise_hello_story_event`
  cross-file hops since #5414 `9d8fd4cc9` and the cross-crate `evaluate_function` condition-evaluator hop since
  #5649 `197a9e245`, which added 18 acquisitions to its row), `footstep_system`'s `PhysicsWorld` /
  `RapierHandles` reads (`footstep_declares_its_physics_reads`, #5556 `ac710964e`), and
  `cross_file_hops_and_get_forms_reach_their_acquisitions` pins that the table's cross-file hops and the
  `get` forms actually surface types (#4994). The scan also counts a `PlayerEntity::get(` call as a read of
  `PlayerEntity` (#6362: the accessor lives in another file, so the turbofish scan would otherwise go blind to
  every player read).
  Sibling gates in `scheduler_access_tests.rs`: `scheduler_access_invariants_hold_on_the_real_schedule`
  (non-vacuous floors: ≥9 parallel systems, ≥7 analysed pairs, then 0 undeclared / 0 conflicts / 0
  unknown), `contract_bearing_exclusives_declare_their_access`, `p2_gameplay_exclusives_declare_non_empty_access`,
  `late_telemetry_declarations_read_all_their_resources`. Boot enforces the same three counts as
  RELEASE `assert_eq!`s in `install_runtime_registries` (#1394/#1602/#2690) — not `debug_assert!`, not a
  log line. Confirm the guards are live: `rg -n '#\[ignore' byroredux/src/scheduler_access_tests.rs byroredux/src/boot/schedule/mod.rs` returns nothing, and `PARALLEL_SYSTEMS.len()` equals the
  `add_to_with_access(` count.
  The `player_controller_system` row's cross-file hops include `raise_kill_story_event` (the drowning death's
  `StoryEvent` write + cell-context reads, #5641 `17a84f442`) and the scripting crate's `note_death`
  (`KillRecords` write plus its `SceneAliasCandidate` / `CreatureActor` / `FormIdComponent` / `FormIdPool` /
  `PapyrusPlayerEntity` reads, #6217 `ef48dbd03`). Since #5647 `eca0c4709` `raise_kill_story_event` ends in the
  scripting crate's `story_manager::raise_story_event`, whose collision path writes the `PendingScriptStoryEvents`
  resource — a cross-crate, non-turbofish hop the table cannot follow. At HEAD that row declares `writes::<StoryEvent>`
  but no `PendingScriptStoryEvents` write (only the dialogue exclusive in `boot/schedule/late.rs` does): a by-hand check.
  **What the guard cannot see** (audit these by hand for every parallel system and any exclusive being
  promoted to parallel): it matches only single-type turbofish `query` / `query_mut` / `resource` /
  `resource_mut` (+ `try_`), `.get` / `.get_mut` / `.has` (#4994) and the generic
  `remove_component` / `insert_component` helpers (#4821) — NOT `query_2_mut::<A, B>`, `resource_2_mut`,
  or types inferred without a turbofish (`world.get(e)` into a typed binding); it does not follow hops into
  a different file unless listed in the table; closures and macro bodies are opaque.
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
  `&mut self` and `Scheduler` is deliberately not a `Resource` (re-entry impossible, #868). The one other
  entry point is `Scheduler::run_exclusive_named` (#5141, `00115f7a5`): the engine's paused frame path runs
  exactly `debug_drain_system` through it (its name is a shared constant asserted against the live schedule, #5292 `8a9989070`) while the pause menu gates `run` (pinned by
  `run_exclusive_named_runs_only_the_named_system`). It bypasses stage order and per-system timing, so any
  new caller must name a system with no same-frame producer/consumer dependency.
  `add_*` warns on duplicate names; `try_add_*` returns `Err(name)` across the flat name space (#312).
- Cross-stage sequencing (a consumer in an earlier stage than its producer) is invisible to the
  analyzer — see `/audit-concurrency` Dim 4 for the pinned cases and the by-hand rule. The same blind spot
  covers same-stage order against a one-frame marker: `HitEvent` producers (melee, NPC strike, force-greet,
  `Weapon.Fire` projectile flight, the VM's `ProcessTrapHit` flush inside the reference/alias script systems)
  must register before all four consumers — `combat_damage_system`, `hit_event_system`,
  `reference_script_hit_system` (#5991) and the ObScript `obscript_hit_system` (split out of
  `obscript_object_system`, #6293 `6208348f7`) — or `event_cleanup_system`
  drains the hit unseen — dart-trap and projectile damage never landed until `ee6468fd1`, now pinned by
  `hit_event_consumers_run_after_every_producer`. A new producer outside that list needs a name in the test.

### 6. Hot-Path Performance Invariants (regression guards)

Paths: `crates/core/src/ecs/systems.rs`, `byroredux/src/systems/{animation,audio,bounds}.rs`, `byroredux/src/components/audio.rs`
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
  Same rule for `Transform` writers feeding propagation: `player_body_facing_system` moved Late → Update
  (#4995, `player_body_facing_runs_in_update_before_propagation`). **A mover writes `Transform`, never
  `GlobalTransform` alone, on a propagation root** — the next propagation rebuilds the global from the unmoved
  local and erases the step (#5373 `f0c683ef1`: the force-greet and eat/sleep walk arms turned in place
  forever; pinned per system by `forcegreet_walk_survives_transform_propagation` /
  `eat_walk_survives_transform_propagation`). Nothing mechanical guards a new mover — check it by hand.
- **Animation scratch** (`byroredux/src/systems/animation.rs`): the `NameIndex.map` refill is in place
  (`clear` + reserve + reinsert; a fresh map costs a ~3 ms stream-in spike, #824); `SubtreeCache` clears
  only when the `Name` count changes (#278); `events` / `seen_labels` scratch is hoisted and
  `clone`d not `mem::take`n (#828). Lock order inside channel apply is content-determined
  (#2399) — do not reintroduce a macro that hides the acquisition order.
- **`FootstepScratch`** (`byroredux/src/components/audio.rs`; `components.rs` is now a thin re-export over per-domain files, #5467 `6ef8976f2`): `mem::take` + restore so `Vec` capacity
  survives; a per-frame `Vec::new` regresses (#932). Placement (Late exclusive after
  `camera_follow_system`) is pinned by `footstep_runs_after_camera_follow_in_late`. Without a `PhysicsWorld` the
  surface list is one `None` per trigger, so each stride still plays the default sound (#5714 `77eea4311`; an empty
  list used to truncate the one-shot loop), pinned by `stride_threshold_fires_one_default_footstep_without_physics_world`.

### 7. Component Lifecycles (load/unload, transient, idempotency)

Paths: `byroredux/src/{streaming,npc_spawn}*`, `byroredux/src/cell_loader/{unload,load,exterior}.rs`, `byroredux/src/systems/cinematic.rs`, `crates/core/src/ecs/components/`, `crates/scripting/src/{events,timer,cleanup}.rs`, `crates/core/src/animation/registry.rs`
First step: `cargo test -p byroredux --bin byroredux -- rapier_release persistent_cell_stamp_tests detach_tests cinematic_retention_tests` (rustc ≥ 1.96) then read `git log --since=<last-report-date> -- crates/core/src/ecs/components`

- **Cell load/unload symmetry** (`streaming/`, `cell_loader/unload.rs`): every component/resource row a
  cell load attaches is removed on unload (no orphan `CharacterController`, `LightFlicker`,
  `RapierHandles`, animation players, `SeatReservations` claims whose furniture or claimant is gone).
  Spawn dispatch is idempotent — one REFR FormId never spawns twice.
- **`CellRoot` stamp discipline** (`stamp_cell_root_range`, `cell_loader/load.rs`): every resumable applier
  stamps only the `[first, last)` range captured inside its own `advance`; a cursor that spans frames claims
  whatever spawned in between. #5483 `5a68e983e`: the persistent-CELL job seeded once and gave the camera,
  player rig and foreground cell to the persistent root — it now reseeds per advance (`begin_advance`,
  `entities_spawned_between_advances_are_not_claimed`). Defence in depth: `unload_cell_inner` refuses any
  victim in `player_owned_entities` (`PlayerEntity`, `ActiveCamera`, the `PlayerBodyRootEntity` subtree),
  stripping the stray `CellRoot` with a warn — that warn is always a producer bug.
- **Cinematic re-adoption** (`retry_cinematic_readoption`): only parentless released-convoy members queue on
  `CinematicReAdoption`; a root's adoption stamps its whole `Children` subtree with the same `CellRoot`
  (#5384 `faf8e5682`, `parented_subtree_rides_the_roots_adoption`), and the player subtree is never queued or
  adopted (#5379 `203be9ed4`, `player_rider_is_never_queued_or_cell_adopted`). The session-replace purge
  (`purge_cinematic_retention_state(world, ctx)`) collects each pending root's `Children` closure (minus the
  player rig) and releases it through `release_entities`, so no subtree orphans and GPU/Rapier handles go with
  it (#5490 `99cd9fd6c`).
- **Detach pass**: `detach_victims_from_surviving_parents` reads every victim's `Parent` under one guard
  (dropped before the `Children` write) and tests membership by binary search over a sorted slice (#5418
  `9bbe9304e`, source-pinned by `detach_victims_takes_one_parent_guard_for_the_whole_sweep`); a per-victim
  `world.get::<Parent>` is the regression.
- **Behavior components stay sparse**: every AI-procedure `*Behavior` / `*State` / terminal marker is
  `SparseSetStorage`. The roster is pinned in the debug registry by
  `roster_tests::every_ai_procedure_behavior_component_is_registered`
  (`crates/debug-server/src/registration.rs`, #4063); teardown completeness
  (`clear_ambient_behavior`, `npc_spawn/ai_package.rs`) and behavior semantics are `/audit-gameplay`.
- **Transient markers** (two-pattern contract, #2672, `crates/scripting/src/cleanup.rs` `contract_tests`):
  Pattern A markers with no single owning consumer (`ActivateEvent`, `HitEvent`, `SplashEvent`, `RippleEvent`,
  `TimerExpired`, `AnimationTextKeyEvents`, the `*Batch` / command markers, …) are removed by
  `event_cleanup_system` (Late exclusive, registered after every consumer — pinned by
  `transient_cleanup_is_the_last_late_exclusive`; a second cleanup site would double-free); Pattern B markers
  (`StoryEvent`, the scene-package batches, …) drain in their one consumer and must not also be listed in
  the cleanup. The cleanup system also resets the Papyrus `VmFrameBudget` (#5763). Late readers
  now include `equipment_appearance_system` and `npc_dialogue_selection_system`. `timer_tick_system` never accumulates negative time.
  `StoryEvent` is one slot per entity; since #5647 `eca0c4709` every engine producer (CLOC, AHEL, KILL) raises it
  through `scripting::story_manager::raise_story_event` — the first event on an entity takes the marker, a second for
  the same entity queues on `PendingScriptStoryEvents`, which the dispatcher drains in the same pass (the marker guard
  is released before the queue is taken, so the two never nest; pinned by
  `two_events_on_one_entity_in_a_frame_both_dispatch`). A producer inserting the marker directly is the regression.
- **`AnimationClipRegistry`** (`animation/registry.rs`): interns by ASCII-lowercased path (#790) so
  streaming does not grow it; `release()` clears a slot but never returns it — no free list, by design
  (#2689), because a released handle may still sit on an `AnimationPlayer` / `AnimationLayer`. Every
  evict/reload strands one empty stub (`stub_slot_count()`, visible not closed). Do not propose a
  free list without addressing that aliasing hazard.
- **No animation controller layer**: `AnimationStack` is the whole sequencing surface; audit its
  lifecycle (no dangling clip refs after unload) rather than looking for a controller above it.
- **Retained `EntityId` fields** (never recycled, so a stale id is a failed lookup, not aliasing): new
  per-actor carriers `NpcSkeletonBones` (bone-name → entity map, retained on living actors and mirrored onto
  the process-lifetime player capsule for mid-life gear import), `AmbientEngagement.target`,
  `PlayerBodyRootEntity`. Check every consumer tolerates a despawned target, and that a cell unload which
  despawns a skeleton also drops or invalidates the map holding it.
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
  handle after unload is a no-op, never a panic or OOB read. Same for `NamedSequences`
  (`animation/named_sequences.rs`, M47.3 `f3380f6c8`): a placed object's sequence-name → clip-handle table,
  shared via `Arc` across every placement of the model and re-stamped at spawn (unsaved), read by ObScript
  `PlayGroup` / `IsAnimPlaying`.
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
- **Layer masks**: `AnimationLayer::bone_weights` + `AnimationStack::bone_index` (runtime-only, `serde(skip)`,
  rebuilt by the owning behavior graph) scale a layer per bone through `bone_layer_weight`; `None` is the
  unmasked path, and a bone the mask does not index counts in full. All three weight sites in
  `sample_blended_transform` (single-layer short-circuit included) must go through it
  (`bone_weights_mask_a_layer_per_bone`).
- **Root motion**: `split_root_motion` applies once per tick and is drained — an undrained
  `RootMotionDelta` integrates every frame. A blended stack's per-tick accumulation-root delta is the
  weight-averaged `sampled_root_motion_delta` across every layer whose clip carries that root, not the
  heaviest layer's alone, and each layer is weighed with `bone_layer_weight` on the accumulation root's channel
  (mask and `clip.weight` included — both the root pick and the average; a masked-off layer used to contribute at
  full weight, #6235 `8c5a718e3`; `animation_stack_root_motion_is_the_weighted_mix_of_its_layers`). Havok
  (FO4) clips also carry `root_yaw` keys, written per tick into the `RootMotionYaw` component beside
  `RootMotionDelta` (the stack's layer-weighted mix; unsaved; #6241 `051179b7b`).
- **Clip sinks**: the loose-NIF, KF and cell-REFR spawn sites attach a clip's non-transform channels and read
  its start phase through `anim_convert::attach_clip_sinks` / `clip_start_phase` (#5590 `d58e7dbad`, pinned by
  a source-scan test) — a hand-rolled fourth copy is the drift.
- **Text keys**: `visit_stack_text_events` (`stack.rs`) emits each key exactly once across a loop
  wrap and still emits when one large `dt` spans several key times.
- **Interpolation** (`interpolation.rs`): `find_key_pair` at t < first / t > last key; quaternion
  shortest-path (missing dot-sign flip = bone spinning the long way). B-splines occur on FNV/FO3 too.

## Process

1. Scope: run each dimension's `First step:`; skim dimensions whose Paths are unchanged.
2. `cargo test -p byroredux-core --features inspect` and `cargo test -p byroredux` (counts live in
   ROADMAP.md; do not pin a number). Every `-p byroredux` invocation here needs the rustc ≥ 1.96
   toolchain recipe in CLAUDE.md (#4466); the default toolchain gives no feedback.
3. Save the report to `docs/audits/AUDIT_ECS_<TODAY>.md`.
