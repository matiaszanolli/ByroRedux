# #2397 — ECS-D2-NEW-01: `SparseSetStorage::remove_entities_erased` is a byte-identical copy of the trait default, and skips the one optimization the override exists to add

- **Severity**: LOW
- **Domain**: ecs
- **Audit**: `docs/audits/AUDIT_ECS_2026-08-07.md`
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2397


- **Severity**: LOW
- **Dimension**: 2 — Storage Correctness (batch-removal path)
- **Location**: `crates/core/src/ecs/sparse_set.rs:172-177`, versus the default at `crates/core/src/ecs/storage.rs:87-92`
- **Status**: NEW

**Description**

`ede92928` introduced `DynStorage::remove_entities_erased` with a default impl (`debug_assert` ascending + per-entity loop) and then added an override on `SparseSetStorage` whose body is textually identical to that default. The trait doc says backends "may override this when they can compact once instead of repeating an expensive structural mutation" — the sparse backend does neither. It forgoes the one cheap win actually available on this path: an `if self.dense.is_empty() { return; }` early-out. `World::despawn_batch` fans the victim slice to every registered storage, and there are 147 `SparseSetStorage<Self>` component declarations against 11 `PackedStorage<Self>` ones — an exterior unload of ~10k victims therefore drives ~1.5M `sparse.get(idx)` probes, the large majority against storages that hold none of the victims.

**Evidence**:

```rust
// storage.rs (default)
fn remove_entities_erased(&mut self, entities: &[EntityId]) {
    debug_assert!(entities.windows(2).all(|pair| pair[0] < pair[1]));
    for &entity in entities { self.remove_entity_erased(entity); }
}
// sparse_set.rs (override) — same assert, same loop, `remove_entity_erased`
// inlined to the identical `<Self as ComponentStorage<T>>::remove` call
fn remove_entities_erased(&mut self, entities: &[EntityId]) {
    debug_assert!(entities.windows(2).all(|pair| pair[0] < pair[1]));
    for &entity in entities { <Self as ComponentStorage<T>>::remove(self, entity); }
}
```

**Impact**

No correctness impact today. Two risks: (a) drift — a future change to the batch contract applied to the default but not the override, or vice versa, would silently diverge for 147 of the 158 component types; (b) the unload-cost optimization the commit was written for is left on the table for the dominant storage class.

**Related**: `ede92928`; `World::despawn_batch` (`world.rs:145-162`); `byroredux/src/cell_loader/unload.rs:245`.

**Suggested Fix**: Either delete the override (the default is already correct and the assert is already there), or keep it and make it earn its place with an `if self.dense.is_empty() { return; }` guard plus a single `structural_gen` bump for the whole batch.

## Completeness Checks
- [ ] **SIBLING**: Check whether `PackedStorage`'s override has the equivalent early-out (it does not, per ECS-D2-NEW-02/03's sibling findings — coordinate fixes)
- [ ] **TESTS**: A benchmark or count-based test confirming the early-out actually skips probing for storages holding none of the victims

---
Filed from `docs/audits/AUDIT_ECS_2026-08-07.md` via `/audit-publish`.
