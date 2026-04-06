# Issue #86: Safety: same-thread query + query_mut deadlocks (no reentrant guard)

**Severity**: HIGH | **Dimension**: Thread Safety | **Domain**: ecs

`std::sync::RwLock` is not reentrant. `world.query::<T>()` followed by `world.query_mut::<T>()` 
on the same thread deadlocks silently. `query_2_mut` guards against this for 2-component queries 
but nothing prevents ad-hoc same-type double-locking.

**Fix**: Debug-mode thread-local tracker: record locked TypeIds per thread, panic on same-type read→write.

## Completeness Checks
- [ ] **LOCK_ORDER**: Verify tracker covers both component and resource locks
- [ ] **TESTS**: Test that same-type double-lock panics in debug mode
