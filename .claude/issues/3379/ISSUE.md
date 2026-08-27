# PHYS-D3-2026-08-27-01: `PersistentCellApplyJob` re-stamps its whole entity range on every yield, duplicating the persistent CELL's unload victim list

- **Issue**: [#3379](https://github.com/matiaszanolli/ByroRedux/issues/3379)
- **Finding ID**: `PHYS-D3-2026-08-27-01`
- **Source report**: `docs/audits/AUDIT_PHYSICS_2026-08-27.md`
- **Audit suite preset**: streaming-deep (2026-08-27)
- **Labels**: `high,physics,memory,bug`

> Immutable snapshot of the issue **as filed** (TD10-001 / #1156). GitHub is authoritative
> for current state — query `gh issue view 3379 --json state`.

---

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
## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix

---
_Filed by `/audit-publish` from `docs/audits/AUDIT_PHYSICS_2026-08-27.md` (audit-suite preset: streaming-deep). Finding ID: `PHYS-D3-2026-08-27-01`._
