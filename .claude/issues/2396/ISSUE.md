# #2396 — ECS-D2-NEW-02: `PackedStorage::remove_entities_erased`'s two load-bearing invariants — sort order and `TRACK_CHANGES` dirty marking — have no test

- **Severity**: LOW
- **Domain**: ecs
- **Audit**: `docs/audits/AUDIT_ECS_2026-08-07.md`
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2396


- **Severity**: LOW
- **Dimension**: 2 — Storage Correctness / change tracking
- **Location**: `crates/core/src/ecs/packed.rs:256-288`; test gap in `crates/core/src/ecs/packed.rs` (`mod tests`) and `crates/core/src/ecs/world_tests.rs:80-124`
- **Status**: NEW

**Description**

The new merge-compaction path is the only removal route used by cell unload (`unload.rs:245` → `despawn_batch` → `remove_entities_erased`), and it re-derives both class invariants by hand: it rebuilds `entities`/`data` from scratch (so sort order is re-asserted, not inherited from `Vec::remove`) and it re-implements `mark_dirty` inline rather than calling the helper. Neither is pinned. `packed.rs`'s own test module has no `remove_entities_erased` test at all. The one integration test, `despawn_batch_removes_sparse_and_packed_rows_once`, uses components that don't set `TRACK_CHANGES` and asserts only presence/absence and `count::<T>()` — never iteration order, never the dirty set.

**Evidence**: `crates/core/src/ecs/packed.rs:275-280`

```rust
if victim_idx < victims.len() && victims[victim_idx] == entity {
    if T::TRACK_CHANGES {
        self.dirty.push(entity);   // hand-inlined mark_dirty, untested
    }
    victim_idx += 1;
    continue;
}
```

`grep -n "remove_entities_erased" crates/core/src/ecs/packed.rs` returns only the definition — no test-module hit.

**Impact**

A regression that dropped the `self.dirty.push` (or applied the wrong `T::TRACK_CHANGES` guard) would be completely silent in CI: `Transform` and `GlobalTransform` are both `PackedStorage` + `TRACK_CHANGES`, and their consumers would still invalidate via their secondary structural keys today — meaning the bug would only surface later, if either consumer is ever narrowed to key on the dirty set alone. A regression that broke the merge's ordering (an unsorted-victims caller in release, where the `debug_assert` is compiled out) would corrupt every subsequent `binary_search` in that storage — silent wrong-component lookups.

**Related**: ECS-D2-NEW-01 (same commit); #1371 (`drain_dirty_into`), which is pinned by a dedicated regression test — this path is the unpinned sibling.

**Suggested Fix**: Add two tests to `packed.rs`'s test module: one asserting `iter()` order is still ascending after `remove_entities_erased` removes a scattered victim set from a >3-element storage, and one on the `Tracked` fixture asserting `take_dirty()` contains exactly the removed ids. Optionally route the inline push through `self.mark_dirty(entity)` so there is one marking site instead of two.

## Completeness Checks
- [ ] **TESTS**: Add both described tests (ascending-order-after-scattered-removal; `take_dirty()` exact-match on a `TRACK_CHANGES` fixture)
- [ ] **SIBLING**: If the inline `self.dirty.push` is routed through `mark_dirty`, verify no double-marking with any other call site

---
Filed from `docs/audits/AUDIT_ECS_2026-08-07.md` via `/audit-publish`.
