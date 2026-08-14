# PHYS-D3-01

Filed: 2026-08-13 · Source: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2865

---

Found by `/audit-physics` Dimension 3 (ECS Sync). Report: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`.

**Severity**: MEDIUM · **Status**: NEW (same bug *class* as the CLOSED #1374 `billboard_system`, unfixed here)
**Location**: `crates/physics/src/sync.rs:744-795` (esp. `:757-771`, `:789-794`)

## Trigger Conditions
Any frame in which >=1 entity carries `RapierHandles` + `RigidBodyData { motion_type: Dynamic }` and its Rapier body still exists. **No motion required** — a body asleep since spawn triggers it identically. Fires from the first frame after a cell with bhk `MO_SYS_DYNAMIC` content finishes loading, and never stops.

## Description
Phase 4 iterates *every* `RapierHandles` row, filters to `Dynamic`, and pushes an update **unconditionally** — no `is_sleeping()` check, no comparison against the current `Transform`, no use of `islands.active_dynamic_bodies()` (which the sibling `dump_awake_fallers` at `:248` already demonstrates is available):

```rust
// crates/physics/src/sync.rs:757-771 — no sleep / no-change gate
for (entity, handles) in handles_q.iter() {
    let Some(body_data) = body_q.get(entity) else { continue; };
    if body_data.motion_type != MotionType::Dynamic { continue; }
    let Some(body) = pw.bodies.get(handles.body) else { continue; };
    let iso = *body.position();                       // asleep bodies included
    updates.push((entity, vec3_from_translation(iso.translation), quat_from_na(iso.rotation)));
}
// :789-794
for (entity, pos, rot) in updates {
    if let Some(t) = tq.get_mut(entity) {             // <- marks dirty, always
```

`PackedStorage::get_mut` calls `mark_dirty(entity)` unconditionally on the mere handing out of `&mut` (`crates/core/src/ecs/packed.rs:125-131`), and `Transform::TRACK_CHANGES = true`. So N dynamic bodies produce N dirty entries per frame regardless of whether anything moved.

This cascades through two systems built around that dirty set:
1. `make_transform_propagation_system` drains it and its fast path requires `transform_dirty.is_empty()` (`crates/core/src/ecs/systems.rs:120-122`). With N >= 1 it is never taken; the incremental branch sorts + dedups and calls `gq.get_mut(e)` for each.
2. That `gq.get_mut` marks `GlobalTransform` dirty, and `world_bound_propagation_system` is that set's *sole* drainer (`byroredux/src/systems/bounds.rs:106`) with a fast path gated on `g_dirty.is_empty()` (`:147-149`). Also never taken.

## Impact
Two O(N log N)-plus-BFS passes per frame that would otherwise early-return, on the exact body population (streamed clutter) the sleep-on-spawn fix exists to keep free. The engine invested heavily in the opposite behaviour: `register_newcomers` spawns dynamic newcomers **asleep** specifically so a streamed exterior does not pin the solver (`sync.rs:592-606`, the "EXTERIOR-FREEZE FIX" comment citing `atw_scheduler=3005ms` and ~3 000 dynamics on one Skyrim exterior frame). Phase 4 then re-arms the *ECS-side* per-frame work those fixes were meant to eliminate, for the same bodies, on the same frames.

Aggravating: in the cell-loading path the writeback target is **invisible**. Bhk colliders spawn as standalone, `MeshHandle`-free ghost entities (`byroredux/src/cell_loader/spawn.rs:1085-1117`) carrying no renderable component, so the `Transform` Phase 4 writes is consumed by nothing. The per-frame cost currently buys zero observable behaviour.

## Suggested Fix
Gate the push. Cheapest correct form: skip bodies where `body.is_sleeping()`, and additionally compare the incoming `iso` against the entity's current `Transform` before taking `get_mut` (an epsilon compare mirroring `push_kinematic`'s own `dt*dt > 1e-6 || dr > 1e-5` gate at `sync.rs:727-732`). Better still, drive the loop from `pw.islands.active_dynamic_bodies()` plus the `body_to_entity` inversion, so the per-frame cost is proportional to awake bodies rather than to all registered ones. Add a regression test asserting the `Transform` dirty set is empty after a tick in which every dynamic body is asleep.

## Related
- #1374 (CLOSED, `medium`/`performance`/`ecs`) — identical shape for `billboard_system` -> `GlobalTransform` -> bounds fast path
- #825, #1371 established the dirty-set design this violates
- Distinct from #2404 (lock ordering in the same function)
