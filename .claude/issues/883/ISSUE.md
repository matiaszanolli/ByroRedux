# Issue #883 (OPEN): CELL-PERF-06: unload_cell does six sequential SparseSet scans over the victim list — collapse into one walk

URL: https://github.com/matiaszanolli/ByroRedux/issues/883

---

## Description

`unload_cell` (`byroredux/src/cell_loader.rs:160-206`) does six independent `for &eid in &victims` loops, one per texture-slot component (MeshHandle, TextureHandle, NormalMapHandle, DarkMapHandle, ExtraTextureMaps, TerrainTileSlot). Each takes a fresh read lock on a different SparseSet and walks the same victim list.

For a 7×7 grid unload (49 cells × ~hundreds of victims each) the redundant SparseSet header walks add up. Today the unload path is rare (only fires past `radius_unload`), so the cost is amortized — but Phase 1b doorwalking will make this hot.

## Evidence

```rust
// cell_loader.rs:160-206 — six independent for &eid in &victims loops
if let Some(mq) = world.query::<MeshHandle>() {
    for &eid in &victims {
        if let Some(mh) = mq.get(eid) { mesh_handles.insert(mh.0); }
    }
}
if let Some(tq) = world.query::<TextureHandle>() {
    for &eid in &victims {
        if let Some(th) = tq.get(eid) { push_tex_drop(th.0, &mut texture_drops); }
    }
}
if let Some(nq) = world.query::<NormalMapHandle>() { /* same shape */ }
if let Some(dq) = world.query::<DarkMapHandle>() { /* same shape */ }
if let Some(eq) = world.query::<ExtraTextureMaps>() {
    for &eid in &victims {
        if let Some(extra) = eq.get(eid) {
            push_tex_drop(extra.glow, &mut texture_drops);
            // ... 5 more slots
        }
    }
}
if let Some(ttq) = world.query::<TerrainTileSlot>() { /* same shape */ }
```

Each `world.query::<T>()` takes a read lock on a different SparseSet header; the per-component overhead is constant, but it's repeated six times for the same victim sweep.

## Why it matters

O(victims × 6) hash lookups instead of O(victims) walks. With the unload path becoming hot under Phase 1b doorwalking, the inversion buys headroom before the doorwalking workload arrives.

## Proposed Fix

Collect every component query once outside the per-component loop, then do a single `for &eid in &victims` that fans out to all six lookups inline:

```rust
let mq = world.query::<MeshHandle>();
let tq = world.query::<TextureHandle>();
let nq = world.query::<NormalMapHandle>();
let dq = world.query::<DarkMapHandle>();
let eq = world.query::<ExtraTextureMaps>();
let ttq = world.query::<TerrainTileSlot>();

for &eid in &victims {
    if let Some(mq) = mq.as_ref() {
        if let Some(mh) = mq.get(eid) { mesh_handles.insert(mh.0); }
    }
    if let Some(tq) = tq.as_ref() {
        if let Some(th) = tq.get(eid) { push_tex_drop(th.0, &mut texture_drops); }
    }
    // ... rest of the slots inline
}
```

Eliminates 5 redundant SparseSet header walks; the per-victim inner cost is unchanged.

## Cost Estimate

Per-cell-unload cost. Today amortized by rare-path firing; becomes hot once Phase 1b doorwalking is active. Lock-acquisition cost is the real axis.

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Audit other multi-component despawn loops in cell_loader.rs / cell_loader_terrain.rs for the same pattern
- [ ] **DROP**: Holding 6 read locks across the victim walk increases lock-hold duration; verify no system writes to any of these SparseSets during cell unload (single-threaded today, so safe — but document the invariant)
- [ ] **LOCK_ORDER**: TypeId-sorted acquisition order MUST be preserved when grabbing all 6 locks at once (per CLAUDE.md invariant #4)
- [ ] **FFI**: N/A
- [ ] **TESTS**: Regression test — unload a cell with 1000 victims, count SparseSet read-lock acquisitions, assert ≤ 6 (was 6 × 1) instead of O(N)

## References

- Audit: `docs/audits/AUDIT_PERFORMANCE_2026-05-06b.md` (CELL-PERF-06)
- Becomes hot once: Phase 1b doorwalking is active
