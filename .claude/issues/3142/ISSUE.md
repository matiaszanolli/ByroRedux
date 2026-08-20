# PERF-D7-01: resident_vwd_refr_cells takes a fresh storage read-lock per VWD entity inside the LOD reconcile loop

**Issue**: #3142 — https://github.com/matiaszanolli/ByroRedux/issues/3142
**Labels**: `low,performance,bug`
**Filed**: 2026-08-20 · comprehensive audit suite
**Report**: `docs/audits/AUDIT_PERFORMANCE_2026-08-20.md`

---

**Severity**: LOW
**Dimension**: Streaming & Cells
**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-08-20.md` (PERF-D7-01)

## Location

`byroredux/src/streaming_helpers.rs:215-225` (`resident_vwd_refr_cells`); called from `update_lod_coverage` (`:174`), which runs on every `reconcile_lod_rings` call including zero-budget ones (`:136-139`)

## Description

The helper holds a `world.query::<VisibleWhenDistant>()` handle but then reads the transform with `world.get::<byroredux_core::ecs::GlobalTransform>(entity)` **inside** the loop:

```rust
fn resident_vwd_refr_cells(world: &byroredux_core::ecs::World) -> Vec<(i32, i32)> {
    let mut cells = std::collections::HashSet::new();
    if let Some(q) = world.query::<crate::components::VisibleWhenDistant>() {
        for (entity, _) in q.iter() {
            if let Some(t) = world.get::<byroredux_core::ecs::GlobalTransform>(entity) {
                cells.insert(streaming::world_pos_to_grid(t.translation.x, t.translation.z));
            }
        }
    }
    cells.into_iter().collect()
}
```

`World::get` (`crates/core/src/ecs/world.rs:333-351`) is **not a cheap probe**: it does a `TypeId` map lookup, constructs a `lock_tracker::TrackedRead` scope guard, acquires and releases the storage `RwLock`, and unwinds the tracker — **per entity**. The accumulator is also a `std::collections::HashSet` built fresh and then `.into_iter().collect()`ed into a `Vec`.

**The function's own doc justifies the wrong thing.** It argues (*"the marker is a sparse ZST … querying it first and looking up `GlobalTransform` per hit is cheaper than a joint query would be"*) — and that reasoning is about **which set to iterate**, and is sound. It does **not** justify re-acquiring the `GlobalTransform` storage lock per hit rather than once outside the loop.

## Evidence

Verbatim at HEAD (`:215-225`). Contrast with the surrounding code's own convention: every sibling in `streaming_helpers.rs` and in `render/static_meshes.rs` acquires a query handle once and calls `.get(entity)` on it — see the #1377 hoist at `static_meshes.rs:163`, **verified intact**.

## Impact

Confined to LOD-reconcile frames — `lod_reconcile_budget_for_frame` returns `None` once `lod_reconcile_pending` clears (`:44-46`), so this is **not** a steady-state per-frame cost.

But reconcile frames are precisely the boundary-crossing frames whose hitch the streaming budget exists to cap, and VWD-flagged placements can number in the hundreds in a dense exterior — so the cost lands exactly where the budget is trying to buy headroom.

## Suggested Fix

- Hoist `let gq = world.query::<GlobalTransform>();` above the loop and use `gq.get(entity)` — the same hoist #1377 applied on the render side.
- Make the accumulator an `FxHashSet` (this is a streaming-path, not a load-time parser, and the keyspace is entity-derived).

## Related

- #1377 / #1805 (the same hoist, applied on the render side)

## Completeness Checks
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved — hoisting widens the `GlobalTransform` read scope to span the `VisibleWhenDistant` query; confirm no write to either storage is taken inside that span
- [ ] **SIBLING**: Same pattern checked in related files — sweep `streaming_helpers.rs` and `app_step.rs` for other in-loop `world.get::<T>()` calls
- [ ] **TESTS**: A regression test pins this specific fix
