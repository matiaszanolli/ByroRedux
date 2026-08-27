# Physics / PHYSAL Audit — 2026-08-27 (Dimension 3 ONLY, streaming-deep preset)

**Scope**: `/audit-physics` **Dimension 3 — ECS Sync: the 4(+1)-Phase Tick**,
run as part of the `streaming-deep` audit-suite preset. Within Dimension 3 the
emphasis was the preset's target: **newcomer registration and body/collider
RELEASE across cell churn** — Rapier handle lifetime vs ECS entity lifetime,
stale handles surviving an unload, double-release, and orphaned bodies after a
partial or interrupted cell load.

**This is NOT a full PHYSAL audit.** Dimensions 1 (Shape Translation),
2 (Step Determinism & Budget), 4 (Ragdoll Articulation & the per-game
constraint seam), 5 (Character Controller & Grounding), 6 (WATAL Buoyancy
Sink) and 7 (Queries, Diagnostics & Cost) were **not run**. No verdict is
offered here on the PHYSAL doctrine claim (is the constraint CInfo decode still
the only per-game seam?) — that lives in Dimension 4 and was last affirmed by
`docs/audits/AUDIT_PHYSICS_2026-08-24.md`.

**Method**: static analysis only. No engine process was launched
(`feedback_no_parallel_engine_launch`). `cargo test -p byroredux-physics`:
**151 passed, 0 failed**.

**Files traced**: `crates/physics/src/sync.rs`, `components.rs`, `world.rs`,
`ragdoll.rs`; `byroredux/src/ragdoll.rs`;
`byroredux/src/cell_loader/{unload,transition,partial,work_budget,load,exterior}.rs`,
`cell_loader/references/mod.rs`, `cell_loader/rapier_release_tests.rs`;
`byroredux/src/{streaming_helpers,app_step,scene}.rs`,
`byroredux/src/scene/world_setup.rs`, `byroredux/src/npc_spawn/resumable.rs`,
`byroredux/src/systems/cinematic.rs`; `crates/core/src/ecs/world.rs`,
`crates/core/src/ecs/resources/mod.rs`; `crates/renderer/src/mesh.rs`.

---

## Executive Summary

| Dimension | CRITICAL | HIGH | MEDIUM | LOW |
|---|---|---|---|---|
| 3 — ECS Sync (streaming emphasis) | 0 | 1 | 0 | 1 |
| 1, 2, 4, 5, 6, 7 | *not run* | | | |

**Two findings, one of them structural.**

The 4-phase tick itself is in good shape and nothing in the sync path regressed
since the 2026-08-24 pass: the `#2867` collect-time idempotency gate, the
read-guards-before-write-guards discipline, the phase ordering, the
`PhysicsWorld`-absent early return, and the ragdoll-activation teardown
(`#1772`) all still hold. The registration half of "newcomers across cell
churn" is clean.

The **release** half is where the streaming lens paid off, and the defect is not
in `crates/physics/` at all — it is in how the cell loader *builds the victim
list* that the physics release path consumes.
`PersistentCellApplyJob::advance` re-stamps its entire entity range from a
**fixed** `first_entity` on every cooperative yield, so
`CellRootIndex[persistent_root]` accumulates the whole prefix once per yield.
Its sibling `ExteriorCellApplyJob` does the same job correctly with a fresh
per-slice `first_entity`, which is what makes the asymmetry legible as a bug
rather than a design choice. Everything downstream of `unload_cell` then
consumes a victim list with duplicates. Rapier survives this by luck
(generational handles make the second `remove_body` a no-op) and
`ItemInstancePool` survives it by an explicit defensive check; the **GPU
refcounts do not**.

The second finding is the defense-in-depth counterpart: the physics release
contract's duplicate-tolerance is *incidental*, undocumented, and has no test.

### What was checked and found clean

- `collect_newcomers` (`crates/physics/src/sync.rs:807-865`) — the
  `handles_q.contains(entity)` gate plus the `#2867` storage-miss refusal still
  make registration exactly-once. Verified against
  `registered_handles_storage_admits_each_newcomer_exactly_once`
  (`sync.rs:1549`) and `missing_handles_storage_registers_nothing_and_does_not_accumulate`
  (`sync.rs:1517`).
- Phase order collect/register → push kinematic → buoyancy → step → pull dynamic
  (`sync.rs:126-190`) unchanged, with the `n_new > 0` buoyancy escape hatch
  intact for a body that streams in already submerged.
- `physics_sync_system` early-returns on absent `PhysicsWorld`
  (`sync.rs:112-115`) — the loose-NIF demo path never enters a phase.
- **Partial / cancelled exterior cell load**: `ExteriorCellApplyJob::cancel`
  (`byroredux/src/cell_loader/exterior.rs:929-935`) routes through the ordinary
  `unload_cell`, and `register_cell_root` is called before the first entity is
  spawned, so a partial apply's colliders are reclaimed. Its per-slice
  `first_entity` (`exterior.rs:1640`, `:1682`) is correct.
- **Half-spawned NPC across a cancellation**: `NpcSpawnJob`'s placement root and
  skeleton bones are created inside the enclosing job's slice, and the enclosing
  job stamps `CellRoot` on every yield — so an actor abandoned mid-bundle is
  still cell-owned and its keyframed bone bodies are released on unload.
- **Abandoned `PersistentCellApplyJob` on a preserved worldspace crossing** (the
  sibling concurrency agent's HIGH finding): its already-spawned entities remain
  cell-owned through the last per-yield stamp, so there is **no** orphaned-Rapier-body
  consequence. Confirmed and deliberately **not** re-reported.
- LOD blocks (terrain / object / placement) and water planes — the three
  reclaim paths that bypass `unload_cell` — carry **no** `CollisionShape`, so
  they have no Rapier objects to strand. The only `CollisionShape` insertion
  sites are `cell_loader/spawn.rs` (cell content, inside the stamped range),
  `scene.rs:1339` (the player capsule, deliberately not cell-owned), and test
  fixtures.
- Ragdoll activation removes the bone's Rapier body **and** both `RigidBodyData`
  and `RapierHandles` (`byroredux/src/ragdoll.rs:414-437`), so a bone is never
  doubly represented and never silently re-registered.
- `scripted_motion_type_system` (`byroredux/src/systems/cinematic.rs:210-255`)
  updates the live Rapier body type alongside `RigidBodyData`, so a motion-type
  change after registration does not desync.
- No `RigidBodyHandle` / `ColliderHandle` is cached anywhere outside
  `RapierHandles` and `Ragdoll`, so there is no third place for a stale handle
  to survive an unload.
- `World::despawn_batch` (`crates/core/src/ecs/world.rs:170-187`) sorts and
  dedups, and entity IDs are never reclaimed — so no handle can alias a
  different entity after a crossing.

### Deliberately not re-filed

- **#3254 (OPEN)** — cinematic unload-retention permanently orphans entities out
  of all cell ownership. Its physics face (a retained cart/horse/actor keeps its
  Rapier body with no cell able to release it) is a *consequence* of #3254, not a
  distinct defect. Fixing #3254 fixes it.
- The sibling concurrency agent's persistent-`CELL` job-lifecycle finding — see
  above; there is no distinct physics consequence to add.

---

## Findings

### HIGH

#### PHYS-D3-2026-08-27-01: `PersistentCellApplyJob` re-stamps its whole entity range on every yield, so the persistent CELL's unload victim list is duplicated N times — and the release paths that consume it are not duplicate-safe

- **Severity**: HIGH
- **Dimension**: ECS Sync — the 4(+1)-Phase Tick (release half / cell churn)
- **Location**: `byroredux/src/cell_loader/exterior.rs:205` (the field),
  `:808` (its one initialisation), `:255-260`, `:273-278`, `:308-313` (the three
  stamp sites); `byroredux/src/cell_loader/load.rs:209-239`
  (`stamp_cell_root_range`, the `entry.extend(first..last)` at `:237`);
  `byroredux/src/cell_loader/unload.rs:138-141` (the victim drain) and its
  consumers at `:219-239`, `:270`, `:284`, `:295`, `:301`
- **Status**: NEW
- **Trigger Conditions**: an exterior worldspace whose **persistent CELL** takes
  more than one cooperative slice to apply — i.e. every interactive exterior
  launch, because `ExteriorBootstrapMode::ForegroundFirst` leaves
  `state.persistent_apply` pending for `advance_streaming_apply` to drive
  frame-by-frame (`byroredux/src/scene/world_setup.rs:776-786`). The damage is
  realised the first time that root is unloaded: an exterior→interior door walk,
  a worldspace crossing that does **not** preserve the persistent root, a
  save-load exterior reload, or shutdown — all of which funnel through
  `drain_streaming_state` → `unload_cell(persistent_root)`
  (`byroredux/src/streaming_helpers.rs:395`, `:423-425`).
  **Benches never see it**: `--bench-frames` selects
  `ExteriorBootstrapMode::FullRadius`, which drives the same job to completion
  under `FrameTimeBudget::unlimited()` in a single call
  (`world_setup.rs:787-805`) — exactly one stamp, no duplicates.
- **Description**: `PersistentCellApplyJob` captures `first_entity` **once**, at
  construction (`exterior.rs:808`, inside
  `begin_worldspace_persistent_cell`). Every subsequent `advance` — on both
  cooperative-yield paths and on the completion path — then stamps the *same*
  fixed start:

  ```rust
  stamp_cell_root_range(
      world,
      self.cell_root,
      self.first_entity,          // ← never advances
      world.next_entity_id(),
  );
  ```

  `stamp_cell_root_range` is overwrite-safe for the `CellRoot` *component* half,
  but its index half is a plain append with no dedup
  (`load.rs:230-238`):

  ```rust
  let entry = idx.map.entry(cell_root).or_insert_with(Vec::new);
  entry.reserve(span);
  entry.extend(first..last);
  ```

  So an entity spawned during slice *k* of an *N*-slice apply is pushed into
  `CellRootIndex[persistent_root]` **N − k + 1** times. Any job that yields even
  once duplicates its whole prefix at least twice (a yield stamp plus the
  unconditional completion stamp at `:308-313`).

  `unload_cell_inner` takes that vector verbatim as its victim list
  (`unload.rs:138-141`) and hands the **same duplicated slice** to five
  consumers. Three tolerate it, two do not:

  | Consumer | Duplicate-safe? | Why |
  |---|---|---|
  | `World::despawn_batch` (`unload.rs:301`) | ✅ | sorts + dedups (`world.rs:174-176`) |
  | `ItemInstancePool::release` (`unload.rs:284`) | ✅ | `cell.take()?` + an explicit `!self.free.contains(&slot)` guard (`crates/core/src/ecs/resources/mod.rs:1127-1136`) |
  | `release_victim_rapier_bodies` (`unload.rs:295`) | ⚠️ **incidentally** | `remove_body` on an already-removed handle returns `false` because rapier's arena is generational; nothing in *our* code prevents the double call — see PHYS-D3-2026-08-27-02 |
  | `collect_victim_gpu_handles` → `mesh_registry.drop_meshes` (`unload.rs:238`) | ❌ | one `mesh_ref_counts` decrement **per pushed handle** (`crates/renderer/src/mesh.rs:912-928`) |
  | `collect_victim_gpu_handles` → `texture_registry.drop_textures` (`unload.rs:270`) | ❌ | same refcount shape |
- **Evidence**:
  - `grep -n "first_entity" byroredux/src/cell_loader/exterior.rs` — the
    persistent job has **one** assignment (`:808`, in the constructor) and three
    reads (`:258`, `:276`, `:311`). Its sibling `ExteriorCellApplyJob` has
    **three** assignments (`:1640`, `:1682`, and `:1474` in the synchronous
    loader), each immediately preceding the work it covers. That is the same
    function doing the same thing correctly, in the same file.
  - `stamp_cell_root_range`'s index half is `entry.extend(first..last)` with no
    dedup and no `contains` check (`load.rs:237`).
  - `unload_cell_inner` does not dedup before use:
    `idx.map.remove(&cell_root).unwrap_or_default()` → `victims` → all five
    consumers.
  - The over-drop is self-evidencing in debug builds: with `mesh_drops`
    containing handle `h` twice, `handle_drop_count[h] == 2` while the true
    refcount is 1, so `freed_meshes` (`unload.rs:223-236`, which requires
    `rc == c`) **excludes** `h` and `accel.drop_blas(h)` is skipped — yet
    `drop_meshes` still takes `h` from 1 to 0 and queues its `VkBuffer`s for
    deferred destruction. `debug_assert_eq!(freed_mesh_count, freed_meshes.len())`
    at `unload.rs:239` is exactly the assertion that catches this, and it can only
    hold if the victim list is duplicate-free.
- **Impact**:
  - *Physics (this dimension)*: `to_remove` is inflated N× and `remove_body` is
    called N× per body. No corruption — the arena's generation check absorbs it —
    but the release path's correctness is resting on a rapier implementation
    detail rather than on our own invariant, and the wasted work is O(duplicates)
    on the highest-latency frame of a transition.
  - *GPU (escalation path, cross-audit)*: refcount over-decrement on every
    persistent-CELL mesh and texture. When the persistent CELL is the sole holder
    the buffer is freed while its **BLAS is deliberately retained** (the
    `rc == c` test failed), leaving an acceleration structure pointing at memory
    queued for destruction — the `BLAS/TLAS with wrong geometry or address` row
    of `_audit-severity.md`. When the handle is **shared with a still-resident
    cell** — the norm for exterior clutter and shared ground textures — the
    refcount reaches zero while a live holder is still drawing it. Debug builds
    trip the `debug_assert_eq!` instead.
  - *Memory*: `CellRootIndex[persistent_root]` grows O(entities × yields);
    bounded per session, but it is the one index the whole ownership model
    depends on being an exact victim set.

  Blast radius is every game, every interactive exterior session, on the most
  common transition in the engine (walk outdoors, then through a door).
- **Related**: PHYS-D3-2026-08-27-02 (the missing idempotency guarantee this
  exploits); `#3254` (the other way an entity's cell-ownership row diverges from
  reality); `#1536` (the precedent: a reclaim set that did not match the thing
  being reclaimed); `CONC-D7-2026-08-27-02` in
  `docs/audits/AUDIT_CONCURRENCY_2026-08-27.md`, which observed the per-yield
  re-stamp and read it as benign ("*they are stamped into the root's `CellRoot`
  range by `stamp_cell_root_range` on every yield*") — correct for the
  entity-reclaim question it was asking, but it did not look at what the repeated
  stamp does to the index.
- **Suggested Fix**: make `PersistentCellApplyJob` track a *slice* start the way
  `ExteriorCellApplyJob` does — replace the constructor-time `first_entity` field
  with a local `let first_entity = world.next_entity_id();` at the top of each
  work unit inside `advance`, so each stamp covers only entities that unit
  created. Belt-and-braces, and worth doing independently: dedup in
  `stamp_cell_root_range`'s index half (or make `unload_cell_inner` sort+dedup
  `victims` once, before any consumer sees it) so no future producer can
  reintroduce this class. A regression test can assert
  `CellRootIndex[root].len()` equals the number of distinct entities after two
  simulated slices, with no `VulkanContext` required.

---

### LOW

#### PHYS-D3-2026-08-27-02: `release_victim_rapier_bodies`' tolerance of a duplicated victim list is incidental, undocumented and untested

- **Severity**: LOW
- **Dimension**: ECS Sync — the 4(+1)-Phase Tick (release half)
- **Location**: `byroredux/src/cell_loader/unload.rs:482-544`;
  `byroredux/src/cell_loader/rapier_release_tests.rs:1-283`
- **Status**: NEW
- **Trigger Conditions**: any caller that hands the function a victim list
  containing the same `EntityId` more than once. PHYS-D3-2026-08-27-01 is one
  such caller today; the point of this finding is that nothing in the contract
  says whether that is allowed.
- **Description**: the function collects one `RapierHandles` (and one `Ragdoll`)
  per *occurrence* of a victim, then calls `pw.remove_body` / `pw.remove_ragdoll`
  once per collected entry:

  ```rust
  for &eid in victims {
      if let Some(h) = handles_q.get(eid) { to_remove.push(*h); }
  }
  ...
  for h in to_remove { pw.remove_body(h.body); }
  ```

  A repeated victim therefore produces a repeated `remove_body` on a handle that
  is already gone. This happens to be safe: rapier's `RigidBodySet::remove`
  matches on the handle's generation and returns `None` for a freed slot, and
  slots are not reused inside the loop because nothing inserts. But that is a
  property of the dependency, not of this code — and `PhysicsWorld::remove_body`
  (`crates/physics/src/world.rs:247-266`) documents only its cascade behaviour
  and its `#2863` wake, not idempotency.

  The two sibling release functions in the same file bracket the gap: the
  `ItemInstancePool` path is explicitly hardened against exactly this
  (*"defensive — duplicate-free is a logic bug elsewhere but we don't want to
  corrupt the arena over it"*, `crates/core/src/ecs/resources/mod.rs:1124-1136`)
  while `collect_victim_gpu_handles` is silently *not* hardened. The physics path
  sits between them with no stated position.

  `rapier_release_tests.rs` — the `#1520`/`#1531` regression guard the audit
  checklist points at — covers seven cases (bodies removed, colliders cascaded,
  non-victims spared, victims without handles, absent `PhysicsWorld`, ragdoll
  bodies/colliders/joints, and the both-components sweep). **None passes a
  duplicated victim.** So the property the code currently depends on is not
  pinned, and a future rapier upgrade or a switch to a non-generational handle
  scheme would break it silently.
- **Evidence**: `grep -n "fn " byroredux/src/cell_loader/rapier_release_tests.rs`
  lists `release_removes_victim_bodies_from_physics_world`,
  `release_cascades_colliders_with_body`, `release_leaves_non_victim_bodies_alive`,
  `release_tolerates_victims_without_handles`,
  `release_is_noop_when_physics_resource_absent`,
  `release_removes_ragdoll_bodies_colliders_and_joints`,
  `release_sweeps_both_ragdoll_and_rapier_handles` — every one is over a
  distinct-entity slice.
- **Impact**: none today beyond the wasted work described in
  PHYS-D3-2026-08-27-01. This is a hardening / test-coverage gap: the release
  contract is the single choke point that keeps `RigidBodySet` bounded across a
  session of cell crossings, and one of its preconditions is currently
  unstated and unverified.
- **Related**: PHYS-D3-2026-08-27-01; `#1520` (the original leak this function
  closed); `#1531` (its `Ragdoll` sibling).
- **Suggested Fix**: add
  *release_is_idempotent_over_a_duplicated_victim_list* to
  `rapier_release_tests.rs` — spawn one collider, sync, call
  `release_victim_rapier_bodies(&mut world, &[e, e])`, assert `body_count() == 0`
  and no panic — and state the precondition in the doc comment ("victims may
  repeat; removal is idempotent"). If the preferred posture is instead
  "victims must be distinct", assert that with a `debug_assert` and fix the
  producer.

---

## Solver Invariant Matrix (Dimension 3 rows only)

| Invariant | State | Evidence |
|---|---|---|
| Phase 1 read guards released before write guards | ✅ VERIFIED | `sync.rs:126-133` — `collect_newcomers` returns an owned `Vec` before `register_newcomers` takes `resource_mut::<PhysicsWorld>()` |
| Newcomer registration exactly-once | ✅ VERIFIED | `handles_q.contains` gate at `sync.rs:848-850` (#2867); pinned by `registered_handles_storage_admits_each_newcomer_exactly_once` |
| Phase order collect/register → push kin → buoyancy → step → pull dyn | ✅ VERIFIED | `sync.rs:126-190`; profiling labels match the spans they time |
| Buoyancy applies force **before** the step | ✅ VERIFIED | `sync.rs:149` precedes `pw.step(dt)` at `sync.rs:157` |
| `PhysicsWorld` absent → no phase runs | ✅ VERIFIED | `sync.rs:112-115` |
| Kinematic push / velocity set call `wake()` | ✅ VERIFIED | `sync.rs:53-109` (the `moved` gate is deliberate, not a missed wake) |
| Cell unload releases every registered handle | ⚠️ **QUALIFIED** | `unload.rs:295` + `:510-544` release everything the victim list names — but the victim list itself is not an exact set (PHYS-D3-2026-08-27-01) |
| Partial / cancelled cell load reclaims its colliders | ✅ VERIFIED | `ExteriorCellApplyJob::cancel` → `unload_cell`; `register_cell_root` precedes the first spawn |
| No Rapier object outlives its ECS entity | ✅ VERIFIED | only `cell_loader/spawn.rs` and `scene.rs:1339` create `CollisionShape`; every LOD/water reclaim path is collider-free |
| No stale `RigidBodyHandle` cached outside `RapierHandles`/`Ragdoll` | ✅ VERIFIED | handle grep over `crates/physics` + `byroredux` |
| Release path is duplicate-safe | ⚠️ **INCIDENTAL** | rapier generational arena, not our invariant; untested (PHYS-D3-2026-08-27-02) |

---

## Known-Open Register

Restated per the skill's requirement; **this pass changed nothing about any of
them and did not re-investigate them**:

1. **`tes_grounding_zero_mass_dynamic_fix`** — Skyrim's mass=0 Dynamic-family
   architecture bodies were reclassified Static (#1832). Closed; the
   door-threshold spawn gap remains open and is Dimension 5 territory, not run
   here.
2. **`interior_spawn_point_fix`** — interiors spawn at the first door's own
   placement; vanilla `coc` has no auto spawn-point logic. Untouched.
3. **`fnv_furniture_sit_needs_transition`** — sit loops carry no pelvis/root
   channel; M42 seat-snap stays behind `BYRO_SANDBOX_SIT`. Untouched.

---

## Cross-Audit Dedup

- **Lock ordering** in `physics_sync_system` → `/audit-concurrency` Dim 5
  (verified clean here, not re-reported).
- **The abandoned `PersistentCellApplyJob`** → `CONC-D7-2026-08-27-02` in
  `docs/audits/AUDIT_CONCURRENCY_2026-08-27.md`. Its physics consequence was
  checked and is **nil**; not re-reported.
- **The GPU-refcount escalation path** of PHYS-D3-2026-08-27-01 (mesh/texture
  over-drop, retained BLAS over a destroyed buffer) belongs to
  `/audit-renderer` Dim 1/3 and `/audit-performance`. It is reported here
  because the defect is in the victim-list producer that the physics release
  path shares, not because the renderer half was audited.
- **`stamp_cell_root_range`** is also the subject of `PERF-D7-2026-08-27-04`
  (`docs/audits/AUDIT_PERFORMANCE_2026-08-27.md`, LOW — batch the `CellRoot`
  inserts). Different half of the same function: that finding is about the
  component insert, this one about the index append. A fix for either should
  land aware of the other.
- **#3254** (cinematic unload retention) — physics face is a consequence; not
  re-filed.

---

## Coverage Gap Statement

Six of the seven `/audit-physics` dimensions were **not executed** in this run:
Shape Translation, Step Determinism & Budget, Ragdoll Articulation & the
per-game constraint seam, Character Controller & Grounding, WATAL Buoyancy Sink,
and Queries/Diagnostics/Cost. No claim in this report should be read as coverage
of them, and no PHYSAL doctrine verdict is issued. The most recent full pass is
`docs/audits/AUDIT_PHYSICS_2026-08-24.md`.
