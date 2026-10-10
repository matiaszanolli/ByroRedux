# #5490: CONC-D5-2026-10-09-01 / SAVE-D4-2026-10-09-01: Regression of #5379 — The session-replace purge despawns only the pending convoy roots with a bare `despawn_batch`

**Labels**: bug, concurrency, ecs, medium, physics, save-load

**Source**: `docs/audits/AUDIT_CONCURRENCY_2026-10-09.md` — finding `CONC-D5-2026-10-09-01` · suite `/audit-suite --preset streaming-deep` (2026-10-09) · HEAD `3bcf6c8e8`

> **Regression of #5379** (closed).

- **Severity**: MEDIUM. The bug is incorrect lifecycle behaviour on a rare but real path. It leaves visible ghost geometry,
  a ghost Rapier collider, and per-occurrence leaked GPU refcounts. It is not per-frame.
- **Dimension**: RwLock Patterns / cell-unload teardown (Dim 5; Dim 6 lifecycle overlap)
- **Location**:
  - `byroredux/src/cell_loader/unload.rs:97-113` (`purge_cinematic_retention_state`: `world.despawn_batch(pending)` at
    `:105`).
  - `byroredux/src/systems/cinematic.rs:552-561` (#5384's parentless-only queue) and `:595` (`pending.extend`).
  - `crates/core/src/ecs/world.rs:170-187` (`despawn_batch`, non-recursive).
  - The canonical release path it bypasses: `unload.rs:372-537` (`release_entities_timed`: GPU drops, item instances,
    `release_victim_rapier_bodies` at `:519`, detach at `:536`).
- **Status**: Regression of #5379, through #5384.
  - #5379's commit `203be9ed4` landed first (Oct 8 22:13). At that point `CinematicReAdoption.pending` held every un-rooted
    member of the release walk, roots and render subtrees alike, so the purge removed the whole convoy.
  - #5384's commit `faf8e5682` came later (Oct 9 15:13; `git merge-base --is-ancestor` confirms the order). It queues only
    PARENTLESS members ("the root's adoption stamps the subtree"), but left the purge untouched. From then on the purge
    removes roots only.
  - The missing GPU and Rapier release dates from #5379 itself.
- **Trigger Conditions**: A scripted convoy (horse + cart + riders) finishes its route outside every loaded cell, so its
  members wait on the re-adoption pending list. A save load or debug load then runs before a cell loads beneath it.
- **Verification Path**: `cargo test`. Extend `purge_despawns_pending_readoption_entities` (`unload.rs:1144`) with a child
  mesh node and `RapierHandles`. Today it uses one childless, meshless, bodiless entity, so it cannot see either half.
- **Description**: The subtree nodes left behind have no `CellRoot`:
  - `cinematic_retained_entities` (`unload.rs:19-49`) retains "complete render hierarchies".
  - `strip_retained_cell_root` (`:161`) strips their `CellRoot` when the home cell unloads mid-tether.

  The session-replace teardown (`drain_streaming_state` → `unload_cells`, `unload_current_interior`) walks only
  `CellRootIndex`, so it cannot reach them. After the purge, their `Parent` names a dead id, so transform propagation never
  refreshes their `GlobalTransform`. They render as frozen geometry in the reloaded world: #5384's own symptom, reopened
  through the save-load path.

  The bare `despawn_batch` also skips everything `release_entities_timed` does before its own `despawn_batch`:
  - mesh, texture and BLAS refcount drops;
  - item-instance release;
  - `release_victim_rapier_bodies`.

  The purge has no `VulkanContext` parameter, so it cannot do any of these. A cart or horse root that carries
  `RapierHandles` leaves its body in `PhysicsWorld`, which is never reset on a session replace (`unload.rs:509-518`
  documents that). The result is an invisible collider where the convoy stood.
- **Evidence**:
  ```rust
  // unload.rs:97-105 — roots only (cinematic.rs:552-561 queues PARENTLESS members), no GPU/physics release
  let pending: Vec<EntityId> = world.try_resource::<CinematicReAdoption>()
      .map(|pending| pending.pending.clone()).unwrap_or_default();
  if !pending.is_empty() { …; world.despawn_batch(pending); … }
  ```
- **Impact**: After such a load:
  - ghost convoy geometry (wheels, horse body parts) frozen at the old position;
  - a phantom collider;
  - mesh, texture and BLAS refs that never reach zero, for the rest of the process.

  The FormIdPair ghost-twin half of #5379 stays fixed, because the roots are still despawned.
- **Related**: #5379, #5384, #3817, #5056, #1520 (why `RapierHandles` must be released before despawn).
- **Suggested Fix**: Hand the convoy to the caller's teardown instead of despawning it in the purge. For example, return the
  pending roots' full `Children` closures and run them through `cell_loader::release_entities(world, ctx, &victims, ..)`;
  both purge call sites in `save_io.rs` and `debug_load.rs` hold `ctx`. Alternatively, stamp them onto a scratch
  `CellRoot` so the drain's `unload_cells` reclaims them through the canonical path.

---

## Merged: SAVE-D4-2026-10-09-01 — the session-replace purge despawns only the pending convoy roots, so their subtree keeps a `Parent` that points at a dead entity, and `validate_world` refuses every later save for the life of the process

*(Same root cause, reported by `AUDIT_SAVE_2026-10-09.md`; filed as one issue.)*

- **Severity**: MEDIUM
- **Dimension**: Save-Side Gates (consequence); the root cause is in Live Load-Apply (teardown completeness)
- **Data-Loss Class**: irrecoverable-write (save-refusal soft-lock: progress made after the load cannot be persisted; every on-disk save stays intact)
- **Location**:
  - `byroredux/src/cell_loader/unload.rs:97-113` (`purge_cinematic_retention_state`: `world.despawn_batch(pending)` at `:105`).
  - `byroredux/src/systems/cinematic.rs:550-563` (#5384: only PARENTLESS members are queued).
  - `crates/core/src/ecs/world.rs:170-187` (`despawn_batch` is non-recursive and touches only the listed ids).
  - `crates/save/src/validate.rs:121-158` (`validate_hierarchy`, the "parent has no Children component" arm at `:152-156`).
  - `byroredux/src/save_io.rs:1030-1047` (`SaveCommand::execute` aborts on any issue).
- **Status**: NEW. It shares its root cause with CONC-D5-2026-10-09-01, but its impact is different and that report does not name it.
  - CONC-D5 covers frozen ghost geometry, mesh/texture/BLAS refcounts that never reach zero, and a phantom Rapier collider.
  - This finding is the save gate: after such a load the player cannot save at all.
- **Trigger Conditions**:
  1. A scripted convoy (horse, cart, riders) releases at its route terminal while a convoy root sits outside every loaded cell, so it waits on `CinematicReAdoption.pending`. The #3817 test models exactly this as "the horse that drove past the loaded ring".
  2. An in-process save load or debug load runs before a cell loads beneath that root.
- **Description**:
  1. Since #5384, the pending list holds parentless roots only, and "the root's adoption stamps the subtree". The subtree nodes lost their `CellRoot` to `strip_retained_cell_root` when the home cell unloaded under retention.
  2. On a session replace, the purge `despawn_batch`es the roots. Each root's `Children` row dies with it, but its direct children keep `Parent(root)`.
  3. Nothing ever reaches those children:
     - the teardown walks only `CellRootIndex`;
     - the #5418 detach pass handles victims whose *parent survives*, which is the opposite direction;
     - a later load's purge finds an empty pending list.
  4. From then on, every `validate_world` call reports one `Hierarchy` error per orphaned child.
  5. Every save path runs `validate_world` and aborts on any issue: F5 quicksave, the pause menu, console `save`, a scripted `RequestSave`, and the remote console. Loading a different save does not clear the orphans, so the refusal lasts until the process restarts.
- **Evidence**:
  - Out-of-repo probe (`/tmp/audit/save/probe`, path deps on `byroredux-core[save]` and `byroredux-save`). The world is root (with `Children`) → child (`Parent`) → grandchild. `validate_world` reports 0 issues before `world.despawn_batch(vec![root])` (what the purge does) and 1 issue after: `[Hierarchy] entity 1: Parent(0) but parent has no Children component`.
  - `purge_despawns_pending_readoption_entities` (`unload.rs:1144`) uses one childless entity, so it cannot see this.
- **Impact**: After that load, every save attempt in the process returns "save ABORTED: N referential-integrity issue(s)". Nothing on disk is corrupted, but any progress made after the load is lost unless the player restarts and reloads. The post-load diagnostic `validate_world` (`save_io.rs:1974-1977`) logs the orphans once, so the cause can be diagnosed.
- **Related**: CONC-D5-2026-10-09-01 (same root cause, different consequence), #5379, #5384, #5056, #3817, and #5310 (the reverse orphan direction, already fixed).
- **Suggested Fix**:
  - Have the purge despawn each pending root's full `Children` closure, or better, hand that closure to the caller's `release_entities` teardown as CONC-D5 suggests. Either way, no child is left pointing at a dead parent.
  - Extend `purge_despawns_pending_readoption_entities` with a child node, and assert that `validate_world` comes back empty after the purge.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other walkers / record decoders / games / call sites)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
