# #2395 — ECS-D2-NEW-03: `PackedStorage::clear_erased` retains full capacity where its `SparseSetStorage` sibling releases it, and `shrink_sparse_tail`'s doc asserts the opposite

- **Severity**: LOW
- **Domain**: ecs
- **Audit**: `docs/audits/AUDIT_ECS_2026-08-07.md`
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2395


- **Severity**: LOW
- **Dimension**: 2 — Storage Correctness / memory footprint
- **Location**: `crates/core/src/ecs/packed.rs:290-298` vs `crates/core/src/ecs/sparse_set.rs:179-191`; doc claims at `crates/core/src/ecs/storage.rs:110-113` and `crates/core/src/ecs/world.rs:279-281`
- **Status**: NEW

**Description**

`#2148` taught `SparseSetStorage::clear_erased` to `shrink_to_fit()` all three vectors. The `PackedStorage` counterpart does `clear()` only. `World::shrink_storages()` doesn't cover it either, because `DynStorage::shrink_sparse_tail`'s default is a no-op and `PackedStorage` doesn't override it. Both doc comments justify that no-op with a claim that is true of length but not of capacity ("`PackedStorage` sizes everything by live count and has nothing to release").

**Evidence**:

```rust
// packed.rs
fn clear_erased(&mut self) {
    self.entities.clear();
    self.data.clear();
    self.dirty.clear();          // no shrink_to_fit on any of the three
}
```

versus `sparse_set.rs:187-189`, which calls `shrink_to_fit()` on `sparse`, `dense`, and `data`.

**Impact**

Bounded, not compounding. After an M45 `World::clear_entities()` + restore into a small interior, the four production packed storages (`Transform`, `GlobalTransform`, `WorldBound`, `SceneFlags`) hold the pre-load exterior peak's capacity — single-digit MB at a 100k-entity peak — until the next `remove_entities_erased`, which re-fits capacity to the then-current length. So it self-heals on the next batch despawn and is not a leak; the concrete defect is the asymmetry with the sparse backend plus two doc comments that state the opposite of what the code does.

**Related**: #2148 (the sparse half of this same concern); `clear_erased_releases_capacity` (`sparse_set.rs:427`) — no packed equivalent.

**Suggested Fix**: Either add `shrink_to_fit()` to `PackedStorage::clear_erased` with a mirror test, or correct both doc comments to say "sized by live count in length; capacity is re-fitted by the next batch removal" so the next reader doesn't take the current wording at face value.

## Completeness Checks
- [ ] **SIBLING**: Match `SparseSetStorage::clear_erased`'s three-vector `shrink_to_fit()` shape if the fix is code (not docs-only)
- [ ] **TESTS**: If capacity release is added, mirror `clear_erased_releases_capacity` (`sparse_set.rs:427`) as a `PackedStorage` test

---
Filed from `docs/audits/AUDIT_ECS_2026-08-07.md` via `/audit-publish`.
