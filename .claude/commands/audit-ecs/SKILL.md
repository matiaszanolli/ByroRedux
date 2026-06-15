---
description: "Deep audit of the ECS — storage backends, queries, world, systems, resources"
---

# ECS Audit

Read `_audit-common.md` and `_audit-severity.md` for shared protocol.

The ECS core is `crates/core/src/ecs/`. Since Session 34's split the module
is one-file-per-concern: `storage.rs` holds only the `Component` /
`ComponentStorage` / `DynStorage` traits + `EntityId`; the two backends live in
`packed.rs` (`PackedStorage`) and `sparse_set.rs` (`SparseSetStorage`).
`world.rs` owns the `RwLock`-per-storage `World`; `query.rs` the guard-owning
query wrappers; `resource.rs` the resource guards; `scheduler.rs` the stage
scheduler; `access.rs` the declared-access conflict analyzer; `lock_tracker.rs`
the deadlock / ABBA detector; `systems.rs` the transform-propagation system.

Dimensions are ordered by ECS blast radius: lock ordering / deadlock first,
then storage correctness, query borrow safety, scheduler declarations, resource
lifetimes, then the cross-cutting lifecycle and hot-path guards.

## Dimensions

### 1. Lock Ordering & Deadlock (HIGHEST blast radius)

A wrong lock order is a HIGH (per `_audit-severity`: "ECS deadlock potential").

- **Same-thread reentrancy**: `lock_tracker` (`lock_tracker.rs`) panics with a
  clear message when a thread takes `write` on a type it already holds (read or
  write), or `read` while holding `write`. The thread-local check runs in BOTH
  debug and release; the global lock-order graph (ABBA, #313) is debug-only.
  Verify every `query` / `query_mut` / `resource` / `resource_mut` site in
  `world.rs` arms a `TrackedRead` / `TrackedWrite` scope, defuses it only AFTER
  the real lock is acquired, and that the wrapper's `Drop` untracks.
- **TypeId-sorted multi-lock acquisition**: `query_2_mut` / `query_2_mut_mut`
  (`world.rs`) and `resource_2_mut` / `try_resource_2_mut` (`world.rs`) acquire
  in `id_a < id_b` order — and set up the *tracker scopes in the same order*
  (the #313 fix: pre-fix the scopes were armed in generic-parameter order, which
  looked like ABBA to the graph when the caller spelled `<B, A>`). A regression
  that arms scopes in parameter order instead of TypeId order re-opens #313.
- **Same-type double-lock panics, never deadlocks**: `query_2_mut` /
  `query_2_mut_mut` / `resource_2_mut` `assert_ne!` on `A == B` with a clear
  message. A silent self-deadlock is the regression.
- **ABBA across rayon workers**: the global graph generalizes the pair guarantee
  to any N-lock hold pattern across the parallel scheduler. Two single-type
  queries acquired in opposite orders on two workers must trip the graph, not
  deadlock. Pin: this is the only protection for ad-hoc N>2 lock holds.
- **Poison-on-panic resolution**: every lock acquisition resolves
  `PoisonError` through `storage_lock_poisoned::<T>()` /
  `storage_lock_poisoned_erased()` / `resource_lock_poisoned::<R>()`
  (`world.rs`) — a post-panic access re-panics loud with the type name, never
  silently reads torn state. `despawn` uses the type-erased variant fed by the
  `type_names` side-table (#466). Removing a poison-resolve site is a finding.

### 2. Storage Correctness

- **SparseSetStorage** (`sparse_set.rs`): swap-remove fixes the sparse pointer
  for the entity moved into the gap (`self.sparse[moved_entity] = Some(dense_idx)`);
  removing the last element takes the no-swap path; insert into an existing
  entity overwrites in place (no duplicate, len unchanged). Pinned by
  `swap_remove`, `remove_last`, `overwrite` in the file's test module.
- **PackedStorage** (`packed.rs`): `binary_search` maintains the sorted-by-entity
  invariant on every insert/remove; `insert_bulk` uses the append + single-sort
  fast path (#467) instead of O(n) per-insert shift; a bulk insert that
  re-sorts must keep the set sorted AND deduplicated.
- **Change tracking (`Component::TRACK_CHANGES`)**: opt-in per-entity dirty set
  (`PackedStorage`, via `mark_dirty` on insert/get_mut/remove) and a monotonic
  `structural_gen` counter (`SparseSetStorage::structural_generation`, bumped on
  insert/remove incl. reparent overwrite). The const is `false` by default so
  non-tracked components pay nothing (branch folds away). Enabled for
  `Transform` / `GlobalTransform`. Audit: `drain_dirty_into` clears `out` then
  drains while *preserving* `self.dirty` capacity (#1371); `take_dirty` hands
  capacity away (0-cap regrow). The dirty set MAY contain duplicates — consumers
  must tolerate that. A storage that forgets to `mark_dirty` on a mutation path
  silently breaks transform propagation's fast path (dim 8).
- **`insert_bulk` debug guard**: `World::insert_batch` (`world.rs`) wraps the
  iterator so the `entity < next_entity` `debug_assert` still fires per item —
  a bulk path that skips it lets unspawned IDs in.

### 3. Query Borrow Safety

- **Guard-owning wrappers**: `QueryRead` / `QueryWrite` / `ComponentRef`
  (`query.rs`) hold the `RwLock*Guard` for the wrapper's lifetime and cache a
  raw pointer downcast ONCE in `new()` (#1367 hot-path fix). The SAFETY argument:
  the cached `*const`/`*mut T::Storage` points into the box the guard keeps
  locked + pinned; no writer can move it while the lock is held. Re-verify each
  `unsafe { &*self.storage }` / `&mut *self.storage` still has the guard field
  alive (the `#[allow(dead_code)] guard` must not be dropped early).
- **`ComponentRef` is the sound replacement for the unsound #35 pattern** —
  it retains the guard rather than returning a raw pointer to dropped storage.
  A regression that drops the guard and hands back a pointer is CRITICAL (UAF).
- **Deref soundness**: `QueryWrite`'s `Deref`/`DerefMut` route through
  `storage()` / `storage_mut()`; `DerefMut` requires `&mut self`, so the borrow
  checker forbids a live `&` and `&mut` into the same storage simultaneously.
- **`query` / `query_mut` return `None` for never-created storage** (no lazy
  empty-storage creation on the read path). `register::<T>()` is the way to
  guarantee a query succeeds before first insert.

### 4. Resource Lifetimes

- `resource()` / `resource_mut()` panic with the type name when the resource was
  never inserted; `try_resource()` / `try_resource_mut()` return `None`.
- `ResourceRead` / `ResourceWrite` (`resource.rs`) downcast through the guard on
  each `Deref` (NOT cached — these are not the #1367 hot path); verify the
  downcast `expect` can't fire (TypeId keys the map).
- Resources are usable from systems via `&self` interior mutability.
- `insert_resource` returns the prior value (downcast back out of the old lock);
  `remove_resource` resolves poison via `resource_lock_poisoned`.
- `try_resource_2_mut` does BOTH existence checks before acquiring EITHER lock
  (#465) — a regression that checks-then-locks-then-checks reintroduces a
  partial-acquire deadlock window.

### 5. System & Scheduler Wiring

- Blanket `System` impl for `Fn(&World, f32)` (`system.rs`); closures and bare
  fns can't override `System::access`, so they declare via the scheduler's
  registration-site override (dim 5b).
- Mutations from a system are visible to later systems in the same `run()`
  (pinned by `mutation_visible_across_stages`).
- Empty scheduler and empty intermediate stages run without panic
  (`empty_scheduler_runs_cleanly`, `empty_stages_skipped`).
- `system_names()` returns stage-order then within-stage (parallel first, then
  exclusive); duplicate names warn on `add_*` but `try_add_*` rejects with
  `Err(name)` across the flat name space (#312).
- Panic policy is **fail-fast by design** (TS-08 / #1412): a panicking system
  aborts the frame and the process; do NOT report "missing `catch_unwind`" as a
  bug — see the `Scheduler::run` doc comment. `run` takes `&mut self` and
  `Scheduler` is intentionally NOT a `Resource` (re-entry is structurally
  impossible, #868).

### 5b. Scheduler Access Declarations (R7 / M27, closed 2026-05-23)

The stages are **`Early` → `Update` → `PostUpdate` → `Physics` → `Late`**
(`Stage` enum, `scheduler.rs`, discriminants `0..=4`, iterated via
`BTreeMap<Stage, _>` `Ord`). There is **no** `ParallelUpdate` or `LateExclusive`
stage — "exclusive" is a *phase within every stage* (`StageData.exclusive`),
not a stage. Exclusive systems run serially after the stage's parallel batch.

- **`Access` (not `SystemAccess`) is the declaration type** (`access.rs`):
  `Access::new().reads::<T>().writes::<U>().reads_resource::<R>()…`. A system's
  declaration is `Some(Access)` or `None` (undeclared). Three states: declared-
  empty ("touches no ECS state"), declared-with-claims, or undeclared (`None`).
  The default for both `System::access()` and the per-entry override is `None`.
- **M27 Phase 1+2** (`a9810d40`): the 12 parallel-stage systems on the engine
  binary declare reads/writes via `Scheduler::add_to_with_access` at the
  registration site in `byroredux/src/main.rs` (closures can't impl
  `System::access`). Any parallel system registered via plain `add_to` (no
  declared access) is a regression.
- **M27 Phase 3** (`05fe2bac`): 4 runtime-mutually-exclusive systems were
  re-staged as **exclusive** to remove structurally-impossible conflicts — most
  notably `player_controller_system` (Stage::Early) declares the *union* of
  `fly_camera` + `character_controller` accesses because it branches on
  `PlayerMode` per frame. `sys.accesses` reports **0 unknown / 0 conflicts**.
- **`AccessConflict` lives in `access.rs`** (re-exported via `ecs::mod`) and has
  EXACTLY three variants: `None`, `Unknown { left_undeclared, right_undeclared }`,
  `Conflict { pairs }`. There is **no** `Parallel` variant (the #1521 wording
  fix). `analyze_pair` returns `Unknown` when one/both sides are undeclared.
  #1394 (`a7e1502b`) added the `undeclared_parallel_count()` accessor on
  `AccessReport` — the migration KPI counting parallel-stage systems still at
  `None` — NOT a reclassification. Driving `undeclared_parallel_count() == 0`
  drives `unknown_pair_count()` to 0 because every parallel pair then has both
  sides declared. Pin: `undeclared_closure_pairs_show_as_unknown`
  (`scheduler.rs`) asserts two undeclared closures yield `unknown_pair_count() == 1`.
- **Exclusive declarations are OPTIONAL and mostly absent** (#1236/#1237,
  `94e78b9f`): `add_exclusive_with_access` / `try_add_exclusive_with_access`
  EXIST so closures/fns *can* declare on the exclusive phase, but the live
  schedule still registers most exclusives via plain `add_exclusive` (e.g.
  `event_cleanup_system`, `audio_system`, `spin_system`, the DLC dispatchers),
  so `undeclared_exclusive_count()` is non-zero by design. The analyzer
  (`access_report`) only pairs parallel-stage systems — exclusives are listed
  but never paired (`exclusive_systems_are_listed_but_not_paired`). Do NOT
  report undeclared exclusives as a conflict; flag only a regression where a
  *parallel* system loses its declaration.
- **#1238 stage-order chain** (`54ea11c0`): `all_five_stages_run_in_order`
  (`scheduler.rs`) registers out of order and asserts the `BTreeMap` `Ord` runs
  `Early..=Late` exactly once. Reordering / merging / inserting a stage without
  updating this test is the regression pattern. (Correct chain:
  `Early → Update → PostUpdate → Physics → Late`.)
- **Regression guard**: `byroredux/src/main.rs` runs
  `debug_assert_eq!(scheduler.access_report().undeclared_parallel_count(), 0)`
  after building the schedule (#1394) — this is the boot guard, NOT a log line.
  Operators inspect contention at runtime via the `sys.accesses` console command
  (reads the `SchedulerAccessReport` resource). A non-zero
  `undeclared_parallel_count` / `known_conflict_count` is an audit finding.

### 6. Unsafe Code Review

- The only `unsafe` in the ECS core is the three cached-pointer derefs in
  `query.rs` (`QueryRead::storage`, `QueryWrite::storage`/`storage_mut`,
  `ComponentRef::Deref`) — all #1367. Each MUST have a SAFETY comment tying
  validity to the live guard. Verify no new unsafe block lacks one (MEDIUM min
  per `_audit-severity`).
- `World::spawn` uses `checked_add` and panics on `EntityId` overflow (#36);
  `despawn` does NOT reclaim IDs (no generational tagging — #372) — document,
  do not "fix" by reusing IDs (silent corruption on dangling `Parent` refs).

### 7. Component Lifecycles (load/unload, transient, idempotency)

- **M40 streaming** (`byroredux/src/streaming.rs`): cell-load attaches
  components, cell-unload removes them — verify no orphaned components after a
  load/unload cycle.
- **M41 NPC spawn** (`byroredux/src/npc_spawn.rs`): ACHR/REFR → entity dispatch
  is idempotent (same REFR FormId never spawns twice).
- **Scripting transient markers** (`crates/scripting/src/events.rs`):
  `ActivateEvent` / `HitEvent` / `TimerExpired` are removed by
  `event_cleanup_system` (registered `add_exclusive(Stage::Late, …)`) — verify
  single-frame lifetime.
- **ScriptTimer** (`crates/scripting/src/timer.rs`): `timer_tick_system`
  decrements per-frame, fires `TimerExpired` on hit — verify no negative-time
  accumulation.
- **Animation controller** (`crates/core/src/animation/controller.rs`):
  controller vs `AnimationPlayer` lifecycle — no dangling clip refs after unload.
- **AnimationClipRegistry** (`crates/core/src/animation/registry.rs`): #790
  dedupes by lowercased path so cell streaming doesn't grow it unboundedly —
  losing case-folding interning leaks one keyframe set per cell load (steady RAM
  growth across exterior streaming).
- **DebugDrainSystem** (`crates/debug-server/src/system.rs`): registered
  `add_exclusive(Stage::Late, …)` (`crates/debug-server/src/lib.rs`) — verify no
  World mutation outside the drain (per-client TCP threads enqueue commands,
  never mutate).
- **AudioWorld** (`crates/audio/src/lib.rs`, M44): `audio_system` runs
  `add_exclusive(Stage::Late, …)`; `OneShotSound` markers are pruned once kira
  reaches `PlaybackState::Stopped` — verify no infinite-marker leak. Spatial
  sub-track handle drop must precede listener handle drop (kira invariant).
- **Particle emitter** (NIFAL typed-block path):
  `byroredux/src/systems/particle.rs::apply_emitter_params` (registered
  `add_exclusive(Stage::PostUpdate, particle_system)`) populates `ParticleEmitter`
  (`crates/core/src/ecs/components/particle.rs`) from
  `ImportedEmitterParams` (`crates/nif/src/import/types.rs`, built by
  `extract_emitter_params` / `extract_emitter_rate` in
  `crates/nif/src/import/walk/mod.rs` from the typed
  `NiPSysEmitter`/`…Ctlr`/`…CtlrData`/`NiPSysGrowFadeModifier` blocks in
  `crates/nif/src/blocks/particle.rs`). Pin the override semantics: authored size
  is `initial_radius × base_scale.unwrap_or(1.0)` (Oblivion has no `base_scale`)
  and color is NOT clobbered — see `apply_emitter_params_size_defaults_base_scale_to_one`
  and `apply_emitter_params_overrides_kinematics_and_size_not_color`. Regression:
  zero-sizing the emitter or overwriting the preset color. See `/audit-nifal`.
- **Character / light-anim** (`byroredux/src/systems/character.rs`,
  `byroredux/src/systems/light_anim.rs`): `character.rs` owns KCC state via
  `byroredux_physics::CharacterController` (+ `RapierHandles`);
  `animate_lights_system` reads `LightFlicker` (`crates/core/src/ecs/components/light.rs`)
  against `LightSource`. Verify no orphaned `CharacterController` / `LightFlicker`
  after a cell load/unload cycle, matching the `streaming.rs` orphan invariant.

### 8. Hot-Path Performance Invariants (regression guards)

- **Lock-tracker held-set collection is `cfg(debug_assertions)`-gated** (#823):
  the `held_others: Vec` built before `record_and_check` in `lock_tracker.rs`
  (`track_read`) is gated as one block — release builds skip the alloc entirely.
  Re-enabling for release rebuilds ~100 small allocs/frame for a no-op.
- **`NameIndex.map` in-place refill** (#824): `animation_system`
  (`byroredux/src/systems/animation.rs`, the `idx.map.clear()` block) refills the
  `HashMap` in place (`clear` + `reserve` + reinsert) instead of `new()` +
  `swap`. The fresh-map pattern costs a ~3 ms cell-stream-in spike.
- **Transform-propagation change detection** (#825 + #1371):
  `make_transform_propagation_system` (`crates/core/src/ecs/systems.rs`) keys a
  cached `roots` set on `(Transform::len(), Parent-len-or-0, next_entity_id())`
  AND tracks `Parent` / `Children` `structural_generation()` plus the drained
  `Transform` dirty set. The FAST PATH skips the whole BFS when the dirty set is
  empty and the full state is unchanged — a static cell with a moving camera
  touches ~1 subtree, not all entities (~250 µs/frame regression at Megaton if
  recomputed every frame). Uses `drain_dirty_into(&mut transform_dirty)` to keep
  the scratch capacity across frames (#1371), NOT `take_dirty`. Any path that
  stops bumping `structural_gen` / `mark_dirty` silently breaks this fast path
  (escalate — wrong `GlobalTransform` is a correctness bug, not just perf).
- **`animation_system` scratch hoisting** (#828): `events` / `seen_labels`
  scratches are hoisted out of the per-entity loop and use `clone` (not
  `mem::take`) so capacity persists; helpers `ensure_subtree_cache` /
  `write_root_motion` / `apply_bool_channels` + the `write_lazy!` macro
  (5 color-target arms) were factored out by `2bdbc36` — DRY-undo drift there is
  a finding.
- **`footstep_system` scratch** (#932): `byroredux/src/systems/audio.rs` writes a
  `FootstepScratch: Resource` via `mem::take` + restore to preserve Vec capacity;
  per-frame `Vec::new` is the regression. (Registered
  `add_exclusive(Stage::PostUpdate, footstep_system)`.)
- **Poison side-table** (#466): `World::despawn` names the offending component
  via the `type_names` side-table; removing it loses the type name in panic
  messages (10× harder bisects).

### 9. NIFAL Canonical Material in the Component Layer

The NIFAL tier resolves PBR scalars once, at the single `ImportedMesh → Material`
boundary, so the renderer never re-classifies per draw. The ECS-owned `Material`
component is the landing zone for that contract. See `/audit-nifal` for the
upstream boundary.

- **Plain-`f32` contract**: `Material` (`crates/core/src/ecs/components/material.rs`)
  carries `metalness: f32` / `roughness: f32` — fully resolved, NOT `Option<f32>`.
  A regression to `Option`/`None` re-introduces per-draw classification (HIGH).
- **Single mutation site**: `byroredux/src/material_translate.rs::translate_material`
  is the SOLE `ImportedMesh → Material` boundary; `Material::resolve_pbr`
  (`crates/core/src/ecs/components/material.rs`) is the only fill-the-gap helper
  (runs the shared `classify_pbr_keyword`, fills only the unset slot). No
  per-draw `classify_pbr` fallback survives in `byroredux/src/render/static_meshes.rs`.
- **`resolve_pbr` idempotent + preserves translator values**: pinned by
  `resolve_pbr_is_idempotent`, `resolve_pbr_preserves_upstream_translator_values`,
  `resolve_pbr_fills_only_missing_slot`, `resolve_pbr_clamps_authored_out_of_range`
  in the `material.rs` test module. Clobbering authored scalars or breaking
  idempotency is a finding.
- **ECS-adjacent producers**: Starfield CDB output (`crates/sfmaterial/`) must
  flow through `translate_material` / `resolve_pbr`; `crates/debug-ui/` (egui
  overlay) must not register or mutate gameplay components.

## Process

1. Read each file in `crates/core/src/ecs/` (paginate the >1500-line ones:
   `resources.rs`, `world_tests.rs`, `scheduler.rs`).
2. Run `cargo test -p byroredux-core` and `cargo test -p byroredux` — verify the
   scheduler/storage/query suites are green (test counts live in ROADMAP, not
   here; do not pin a number).
3. Check each dimension top-down (lock ordering first).
4. Save report to `docs/audits/AUDIT_ECS_<TODAY>.md`.
