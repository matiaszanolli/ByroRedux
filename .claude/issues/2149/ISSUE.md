**Severity**: LOW · **Dimension**: 1 — Lock Ordering & Deadlock (tracker hygiene)
**Source**: `docs/audits/AUDIT_ECS_2026-07-25.md` (ECS-2507-03)
**Status**: NEW

## Description
`World::query` calls `scope.defuse()` **before** constructing `QueryRead`, and
`QueryRead::new` performs
`downcast_ref::<T::Storage>().expect("storage type mismatch (bug in World)")`.
If that `expect` ever fired, the tracker row would already be un-owned by the
`TrackedRead` scope and not yet owned by a `QueryRead` (whose `Drop` is the
only untrack path), leaving a stale entry in the thread-local `LOCKS` map. A
later acquisition on the same thread after a `catch_unwind` would then report
a spurious "ECS deadlock detected". This is the exact failure mode #137 fixed,
and `World::get` (`world.rs:288-300`) gets the ordering right — it defuses
only inside the `Some` arm, after `ComponentRef::new` has returned. `query`,
`query_mut`, `query_2_mut`, `query_2_mut_mut` are inconsistent with it.

## Evidence
```rust
// world.rs:394-397 — defuse precedes the fallible construction
let scope = lock_tracker::TrackedRead::new(type_id, std::any::type_name::<T>());
let guard = lock.read().unwrap_or_else(|_| storage_lock_poisoned::<T>());
scope.defuse();
Some(QueryRead::new(guard, type_id))   // <- .expect() inside
```

vs. the correct shape in `World::get`:
```rust
match ComponentRef::new(guard, entity, type_id) {
    Some(cr) => { scope.defuse(); Some(cr) }
    None => None,   // scope drops → untrack
}
```

## Impact
None in practice — the downcast can only fail if `World.storages` maps a
`TypeId` to a storage that is not `T::Storage`, which is impossible by
construction (`storage_write`/`register` create `T::Storage::default()`
under `TypeId::of::<T>()`, and two distinct types cannot share a `TypeId`).
This is a defense-in-depth / consistency gap on an unreachable path, not a
live bug. The poison path *is* handled correctly (the panic happens inside
`unwrap_or_else`, before `defuse`).

## Suggested Fix
Move the downcast + `expect` out of `QueryRead::new` / `QueryWrite::new` into
a fallible `try_new` and defuse only on success, or simply reorder so
`defuse()` runs after the wrapper is constructed
(`let q = QueryRead::new(guard, type_id); scope.defuse(); Some(q)`), matching
`World::get`.

## Related
#137 (`TrackedRead`/`TrackedWrite` RAII scopes), `lock_tracker::is_clean()`
test helper.

## Completeness Checks
- [ ] **LOCK_ORDER**: Confirm the reordering doesn't change tracker-scope acquisition order relative to the ABBA graph (it shouldn't — same scope, just later defuse)
- [ ] **SIBLING**: Apply the same fix consistently across `query`, `query_mut`, `query_2_mut`, `query_2_mut_mut` (all four sites listed) — not just one
- [ ] **TESTS**: A regression test (or `lock_tracker::is_clean()` assertion) pins that the tracker scope is still armed at the point of the fallible downcast
