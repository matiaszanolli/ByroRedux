# Issue #467

E-04: PackedStorage insert/remove is O(n) via Vec::insert / Vec::remove middle-of-vec shift

---

## Severity: Low (perf hardening, not correctness)

**Location**: `crates/core/src/ecs/packed.rs:38,47`

## Problem

```rust
// line 38:
self.entities.insert(idx, entity);
self.data.insert(idx, component);
// line 47:
self.entities.remove(idx);
Some(self.data.remove(idx))
```

`Vec::insert(idx, ..)` and `Vec::remove(idx)` shift every element after `idx`. For `Transform` with thousands of entities, each insert/remove is O(n). The sort invariant is maintained correctly — this is a perf note.

## Impact

Measurable only during bulk scene load (BSA/NIF import into a fresh cell). Steady-state frames don't hit this because Transform is rarely mutated structurally.

## Fix

If profiling shows a hotspot during cell load:

1. Add a batched `extend_then_sort` path for bulk import:
   ```rust
   pub fn extend_sorted<I: IntoIterator<Item = (EntityId, T)>>(&mut self, iter: I) {
       self.entities.reserve(iter.size_hint().0);
       self.data.reserve(iter.size_hint().0);
       for (e, c) in iter { self.entities.push(e); self.data.push(c); }
       // Sort both vecs together by entity id.
       let mut indices: Vec<usize> = (0..self.entities.len()).collect();
       indices.sort_unstable_by_key(|&i| self.entities[i]);
       // ... rearrange via indirection
   }
   ```
2. Call this from `AssetProvider::instantiate_scene` on bulk NIF import.

**Do NOT** switch the point-insert path to swap-remove / swap-insert — that would break the binary_search sort invariant that query iteration depends on.

## Completeness Checks

- [ ] **TESTS**: Existing `packed_storage_*` tests pass unchanged
- [ ] **PROFILE**: Bench cell load (Megaton + FNV Prospector) before and after — confirm a measurable win
- [ ] **SIBLING**: Verify the batched path is called only for fresh/empty storages where order doesn't matter pre-sort
- [ ] **DOCS**: Note the O(n) per-insert cost in `PackedStorage` doc comment with guidance on bulk load

Audit: `docs/audits/AUDIT_ECS_2026-04-19.md` (E-04)
