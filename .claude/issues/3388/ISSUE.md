# PERF-D7-2026-08-27-04: `stamp_cell_root_range` inserts `CellRoot` one entity at a time where `World::insert_batch` fits exactly

- **Issue**: [#3388](https://github.com/matiaszanolli/ByroRedux/issues/3388)
- **Finding ID**: `PERF-D7-2026-08-27-04`
- **Source report**: `docs/audits/AUDIT_PERFORMANCE_2026-08-27.md`
- **Audit suite preset**: streaming-deep (2026-08-27)
- **Labels**: `low,performance,ecs,bug`

> Immutable snapshot of the issue **as filed** (TD10-001 / #1156). GitHub is authoritative
> for current state — query `gh issue view 3388 --json state`.

---

- **Severity**: LOW
- **Dimension**: Streaming & Cells
- **Location**: `byroredux/src/cell_loader/load.rs:209-239`
- **Status**: NEW
- **Description**: The function stamps one component type over a contiguous
  `first..last` entity range — the textbook batch shape. It uses per-entity
  `world.insert(eid, CellRoot(cell_root))`, so every entity pays the full
  `World::insert` preamble: a `TypeId` lookup in `self.type_names`, a second in
  `self.storages` (both `std` SipHash maps), a `RwLock::get_mut`, and an
  `as_any_mut().downcast_mut()` — around six times the cost of the O(1)
  `SparseSetStorage::insert` it wraps.

  `World::insert_batch` (`crates/core/src/ecs/world.rs:238-263`) exists and
  documents itself as being for precisely this ("amortizes the per-call HashMap
  lookup + `downcast_mut` across the batch… prefer this when a loader / import
  path has a natural 'collect all Transforms then all GlobalTransforms' shape").
  The #512 note attached to it correctly says the cell loader's *scatter-shot*
  per-entity multi-type pattern does not benefit — but this site is not that
  pattern, it is the batch pattern, and the function's own second half already
  reaches for the batched form on the index side (`entry.reserve(span)` +
  `entry.extend(first..last)`, `:229-231`, landed as #885).
- **Evidence**: `load.rs:215-222`
  ```rust
  for eid in first..last {
      world.insert(eid, CellRoot(cell_root));
  }
  ```
  immediately above `entry.extend(first..last)` for the `CellRootIndex` half.
- **Impact**: called once per phase of every cell apply
  (`exterior.rs:1604, 1652, 1663, 1699` plus the interior loader), over the whole
  spawned range, so the total is O(entities in the cell) with the ~30 ns/entity
  preamble on top of the ~5 ns insert. Order of ~70 µs per 2 000-entity cell —
  small, but it is a strictly-dominated call in the boundary path and the
  replacement is one line.
- **Related**: #885 (the sibling index half of this same function, already
  batched); #512 (the migration note that scopes when `insert_batch` helps).
- **Suggested Fix**: `world.insert_batch((first..last).map(|eid| (eid, CellRoot(cell_root))));`

---
## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix

---
_Filed by `/audit-publish` from `docs/audits/AUDIT_PERFORMANCE_2026-08-27.md` (audit-suite preset: streaming-deep). Finding ID: `PERF-D7-2026-08-27-04`._
