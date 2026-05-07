# Issue #882 (OPEN): CELL-PERF-05: per-mesh world.resource_mut::<StringPool>() inside spawn loop churns the write lock

URL: https://github.com/matiaszanolli/ByroRedux/issues/882

---

## Description

`spawn_placed_instances` (`byroredux/src/cell_loader.rs:2150-2154`) acquires a fresh `world.resource_mut::<StringPool>()` write lock for every mesh's `Name(sym)` insert, and a fresh `world.resource::<StringPool>()` read lock for every mesh's resolver block (`:2044`). Megaton has hundreds of write-lock acquisitions inside one cell-load loop.

The pattern from #523 (batched commit at `load_references` end) wasn't extended down into `spawn_placed_instances`.

## Evidence

```rust
// cell_loader.rs:2150-2154 — write lock per mesh in the placement loop
if let Some(ref name) = mesh.name {
    let mut pool = world.resource_mut::<byroredux_core::string::StringPool>();
    let sym = pool.intern(name);
    drop(pool);
    world.insert(entity, Name(sym));
}
```

```rust
// cell_loader.rs:2044 — read lock per mesh + 8 resolves below it, then drop
let pool_read = world.resource::<byroredux_core::string::StringPool>();
let resolve_owned = |sym: ...| -> Option<String> { ... };
```

The `pool` is uncontested today (single-threaded loop) so each acquisition is fast. But each one is still an atomic CAS + drop pair. Once **#879 (CELL-PERF-01)** lands and placements share GPU mesh handles, the per-mesh lock churn becomes the next-most-visible cost.

## Why it matters

Megaton-scale cells trigger hundreds of `intern()` calls inside a single cell-load critical path. The existing #523 batching pattern (gather `pending_new` / `pending_hits` accumulator, single commit at end) demonstrates the right shape for this code path.

## Proposed Fix

Accumulator pattern mirroring #523:

```rust
// Phase 1 — gather (no locks held across the spawn loop)
let mut pending_names: Vec<(EntityId, &str)> = Vec::with_capacity(num_placements);
for mesh in imported {
    // ... mesh handling without StringPool
    if let Some(ref name) = mesh.name {
        pending_names.push((entity, name.as_str()));
    }
}

// Phase 2 — commit (single write lock for the cell)
{
    let mut pool = world.resource_mut::<StringPool>();
    let symbols: Vec<(EntityId, FixedString)> = pending_names
        .into_iter()
        .map(|(eid, n)| (eid, pool.intern(n)))
        .collect();
    drop(pool);
    for (eid, sym) in symbols {
        world.insert(eid, Name(sym));
    }
}
```

One write lock per cell instead of per mesh. The read-side resolver block at `:2044` admits the same restructuring.

## Cost Estimate

Per-mesh atomic CAS pair × ~hundreds of meshes/cell. Below the wall-clock signal floor today (uncontested lock); becomes visible once #879 removes the upload-stall noise floor.

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Audit other cell-loader call sites for per-iteration `resource_mut::<StringPool>()` patterns; #523 batched the top-level load_references but this child loop wasn't covered
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: Single write lock acquisition inverts the current pattern of "many short locks" to "one long lock" — verify no inner `world.insert` requires StringPool concurrently (the current single-threaded structure means yes, but make this an explicit invariant)
- [ ] **FFI**: N/A
- [ ] **TESTS**: Regression test — cell load that touches 100 named meshes; count `StringPool::intern_with_lock_acquired` invocations, assert ≤ N+constant rather than O(N)

## dhat Gap

Zero allocation impact (StringPool entries are interned regardless); pure CPU lock-acquisition cost. Profile via `tracing` — NOT dhat.

## References

- Audit: `docs/audits/AUDIT_PERFORMANCE_2026-05-06b.md` (CELL-PERF-05)
- Builds on: #523 (batched `pending_new` / `pending_hits` pattern)
- Becomes hot once: #879 (CELL-PERF-01) removes the upload-stall noise floor
