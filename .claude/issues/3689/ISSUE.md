# #3689 — PERF-D7-2026-08-30-04: `PackedStorage::remove_entities_erased` reallocates and moves every *surviving* row per unload batch, so eviction cost is O(all resident rows), not O(victims)

**Severity**: LOW · **Dimension**: Streaming & Cells
**Location**: `crates/core/src/ecs/packed.rs::PackedStorage::remove_entities_erased`

## Fix

The #2397 merge-compaction drained both backing `Vec`s (`entities`, `data`)
via `std::mem::take` into two fresh `Vec::with_capacity(old_len)` buffers,
pushing every *surviving* row into the new buffers. Correct output, but it
paid `2 × sizeof(row) × live_rows` of allocate-plus-move on every call —
for each `PackedStorage` component type, on every unload batch,
regardless of how few entities the victims actually owned. `despawn_batch`
calls this once per registered storage, so a three-cell boundary eviction
that touches a handful of entities still copied the retained 90-plus
percent of `Transform`/`GlobalTransform`/`SceneFlags`/`WorldBound` out and
back, four times.

Replaced the drain-into-two-fresh-Vecs shape with an in-place read/write
cursor over the existing buffers — the `Vec::retain` shape, driven by the
sorted victim cursor so `mark_dirty` still fires exactly once per removed
entity:

```rust
let mut victim_idx = 0usize;
let mut write = 0usize;
for read in 0..self.entities.len() {
    let entity = self.entities[read];
    while victim_idx < victims.len() && victims[victim_idx] < entity {
        victim_idx += 1;
    }
    if victim_idx < victims.len() && victims[victim_idx] == entity {
        self.mark_dirty(entity);
        victim_idx += 1;
        continue;
    }
    if write != read {
        self.entities.swap(write, read);
        self.data.swap(write, read);
    }
    write += 1;
}
self.entities.truncate(write);
self.data.truncate(write);
```

`Vec::swap` moves a kept row backward into the next free slot without
requiring `T: Clone`/`Default` (works for any `T`, matching this storage's
existing generic bound). Positions before `write` are already finalized
and never revisited; the leftover single-copy tail past `write` (victims
plus whichever retained rows their slots were last swapped from) is
dropped in place by `truncate`. Zero allocations, same output order, same
single merge pass over sorted `victims`.

## SIBLING (issue's own checklist item)

`SparseSetStorage`'s own bulk-remove path (the #2148 sibling this issue
names) already operates on a `HashMap`-backed storage with no equivalent
"rebuild two parallel Vecs" shape — nothing to change there. `insert_bulk`
(same file) still allocates fresh `new_entities`/`new_data` Vecs for its
own reorder pass; left untouched per its own doc comment, which already
states why (non-Copy `T` reorder without an auxiliary bitset, and it only
runs at cell-load boundaries, not the streaming-eviction hot path this
issue is about).

## TESTS (issue's own checklist item)

The issue's own suggested-fix note ("Both existing tests ... pin the
observable contract and should pass unchanged") held: both
`remove_entities_erased_preserves_ascending_order` and
`remove_entities_erased_marks_exactly_the_removed_ids_dirty` pass
unmodified against the new implementation (updated only their doc
comments, which described the old rebuild-from-scratch shape).

Added `remove_entities_erased_does_not_reallocate` — the concrete,
testable signature of "in place, not two fresh allocations": captures
`entities`/`data`'s buffer pointers and capacities before a removal,
asserts both are bit-for-bit unchanged after.

**Reintroduce-and-revert verification**: temporarily restored the old
drain-into-two-fresh-Vecs implementation — confirmed
`remove_entities_erased_does_not_reallocate` failed (different pointer,
as expected — a fresh `Vec::with_capacity` never returns the same
allocation as the original growth-doubled buffer), while the other two
existing tests still passed unchanged (same observable contract, per the
issue's own note). Restored the fix and reran — all 21 tests in
`packed::tests` pass again.

## Verification

- `cargo check -p byroredux-core --tests`: clean.
- `cargo test -p byroredux-core --lib packed::`: 21 tests passing, 0
  failing (+1 new).
- `cargo test -q -p byroredux-core`: 728 tests passing (+1), 0 failing.
- `cargo test -q --no-fail-fast` (full workspace): **7107 passing, 0
  failing**.
