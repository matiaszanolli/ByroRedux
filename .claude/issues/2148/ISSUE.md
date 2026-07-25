**Severity**: MEDIUM · **Dimension**: 2 / 8 — Storage Correctness + Hot-Path / Memory
**Source**: `docs/audits/AUDIT_ECS_2026-07-25.md` (ECS-2507-02)
**Status**: NEW

## Description
`SparseSetStorage.sparse: Vec<Option<u32>>` is indexed directly by `EntityId`
and grown with `resize(idx + 1, None)` on insert. It is **never** truncated or
shrunk: `remove` writes `None` into the slot but leaves the `Vec` length
untouched, and `clear_erased` calls `.clear()` (which keeps capacity). Because
entity IDs are deliberately never reclaimed (`World::despawn`, #372) and
`next_entity` only grows, every sparse-set storage that ever receives an
insert near the current high-water mark permanently retains
`8 bytes × high_water_mark` of RAM regardless of how few components are
actually live. There are **122** `SparseSetStorage<Self>` component
declarations in the workspace; the ones attached to nearly every spawned
entity (`Name`, `FormIdComponent`, `MeshHandle`, `Parent`, `Children`,
`RenderLayer`, `CollisionShape`, …) all track the global high-water mark.

## Evidence
```rust
// sparse_set.rs:60-63 — only growth, no counterpart shrink
if idx >= self.sparse.len() {
    self.sparse.resize(idx + 1, None);
}
```

```rust
// sparse_set.rs:88 — remove clears the slot but not the length
self.sparse[idx] = None;
```

```rust
// sparse_set.rs:147 — clear() retains capacity
self.sparse.clear();
```

`grep -rn "shrink" crates/core/src/ecs/*.rs` returns nothing — there is no
compaction API anywhere in the ECS core. Cell unload
(`cell_loader/unload.rs:199`) goes through `World::despawn`, which never
touches `next_entity`.

## Impact
RAM growth proportional to (cumulative entities ever spawned) × (number of
sparse component types touched), *independent of live entity count*.
`Option<u32>` has no niche, so each slot is 8 bytes: a 2M-ID high-water mark
costs ~16 MB per affected storage, i.e. a few hundred MB across a dozen
commonly-attached sparse components. This is exactly the shape of a long
exterior-streaming session (repeated cell load → despawn → load at
ever-higher IDs) and it is invisible to `cargo test`. Against the
`docs/engine/memory-budget.md` "under ~4 GB total" target this is material,
though it is a slow accumulation, not a per-frame leak.

## Suggested Fix
Two independent, low-risk mitigations. (a) Halve the per-slot cost by
replacing `Vec<Option<u32>>` with `Vec<u32>` plus a `u32::MAX` sentinel
(4 bytes/slot, same O(1) semantics). (b) Add a `shrink_sparse_tail()` that
truncates trailing `None` slots plus a `shrink_to_fit()`, exposed on
`DynStorage` and invoked once per cell-unload from `unload_cell` — cheap (a
backwards scan) and only runs at load boundaries. Also make `clear_erased`
call `shrink_to_fit()` so a save-load actually returns the memory.

## Related
#372 (IDs never reclaimed — the documented decision this interacts with; do
**not** "fix" by reusing IDs). `PackedStorage` is unaffected (its
`entities`/`data` are sized by live count).

## Completeness Checks
- [ ] **SIBLING**: If a shrink hook is added, verify it composes correctly with `PackedStorage` (unaffected) and doesn't disturb dense-index invariants
- [ ] **TESTS**: A regression test pins RAM/len behavior across a despawn-heavy load/unload cycle (e.g. `sparse` len bounded relative to live entity count, or a shrink hook actually truncating trailing `None`s)
