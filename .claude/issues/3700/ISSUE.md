# #3700 — ECS-2026-08-30-D8-01: no hierarchy walk in the engine bounds its traversal, so a Parent/Children cycle hangs the frame with no diagnostic

*Filed 2026-08-30 from `docs/audits/`. Immutable snapshot of the issue as filed (TD10-001 / #1156); GitHub is authoritative for current state.*

**Severity**: MEDIUM · **Dimension**: Hot-Path Performance Invariants / robustness
**Location**: `crates/core/src/ecs/systems.rs` (`make_transform_propagation_system` BFS drain, ~:221-252); siblings at `byroredux/src/systems/bounds.rs` (~:284-289 parent climb, ~:302-319 post-order stack walk); construction path `byroredux/src/helpers.rs` (`add_child`, ~:139-151)
**Source**: `docs/audits/AUDIT_ECS_2026-08-30.md` (ECS-D8-01)

## Description

Three per-frame hierarchy walks share one unguarded shape — none carries a visited set, a depth cap, or an iteration budget:

1. `make_transform_propagation_system`'s BFS drain — `while let Some(entity) = queue.pop_front()` re-enqueues `cq.get(entity)`'s children unconditionally.
2. `make_world_bound_propagation_system`'s parent-chain climb — `while let Some(p) = parent_q… { cur = p; }` with no termination condition other than a missing `Parent`.
3. That system's post-order stack walk.

Nothing upstream prevents a cycle from existing. `add_child` pushes unconditionally — no dedup, no self-parent rejection — and `World::insert(child, Parent(p))` performs no acyclicity check. The save path's `validate_world` (`crates/save/src/validate.rs`) checks that `Parent` <-> `Children` agree and that neither points past `next_entity`, but **not** that the graph is acyclic; and on the load side `restore_world` (`crates/save/src/driver.rs`) runs it through `log_validation_warnings` — a warning, after the world is already populated, not a rejection.

## Evidence

```rust
// crates/core/src/ecs/systems.rs — no visited set, no depth bound
while let Some(entity) = queue.pop_front() {
    let Some(parent) = pq.get(entity) else { continue; };
    // ...
    if let Some(children) = cq.get(entity) {
        queue.extend(children.0.iter().copied());
    }
}
```

```rust
// byroredux/src/helpers.rs — no dedup, no self-parent guard
pub(crate) fn add_child(world: &mut World, parent: EntityId, child: EntityId) {
    // ...
    if has_children {
        let mut cq = world.query_mut::<Children>().unwrap();
        cq.get_mut(parent).unwrap().0.push(child);
```

## Impact

A bidirectionally-consistent cycle reachable from a propagation root produces an unbounded loop inside a `Stage::PostUpdate` system: the process hangs with no panic, no log line, and no watchdog — strictly worse than the fail-fast crash the scheduler's panic policy (#1412) deliberately chose everywhere else. A duplicated `Children` entry is the milder case: exponential re-walk of that subtree rather than a hang.

**Honest framing** (from the report): no live construction path was found. `world.remove::<Parent>` appears only in tests, and every production `add_child` caller passes a freshly-spawned child. This is a defence-in-depth gap on a corrupt or hand-edited save, or on a future spawn-path bug — not a reproducible defect today.

## Suggested Fix

Cheapest correct guard is a per-frame iteration budget on the BFS drain (`queue` pops <= `Transform::len()` + total `Children`, then `log::error!` and bail) — O(1) cost, converts a hang into a diagnosable frame. Adding an acyclicity pass to `validate_world` would additionally stop a corrupt save at the door instead of after `restore_world` has cleared the live world.

## Completeness Checks
- [ ] **SIBLING**: All three walks (transform BFS, bounds parent climb, bounds post-order) get the same bound
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test builds a `Parent`/`Children` cycle and asserts the frame terminates with a diagnostic
