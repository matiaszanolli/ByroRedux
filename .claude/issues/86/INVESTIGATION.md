# Issue #86 Investigation

## Problem
`std::sync::RwLock` is not reentrant. If a system does:
```rust
let q = world.query::<T>();       // read lock
let q2 = world.query_mut::<T>();  // write lock → deadlock!
```
The second call silently deadlocks on the same thread. No panic, no error.

## Affected types
- `QueryRead`, `QueryWrite`, `ComponentRef` (component locks in query.rs)
- `ResourceRead`, `ResourceWrite` (resource locks in resource.rs)
- All World methods: `query`, `query_mut`, `get`, `resource`, `resource_mut`,
  `try_resource`, `try_resource_mut`, `query_2_mut`, `query_2_mut_mut`, `resource_2_mut`

## Fix
Thread-local tracker, debug-only:
1. New `lock_tracker.rs`: `thread_local!(HashMap<TypeId, (read_count, has_write)>)`
2. Guard types get `TypeId` field, register on `new()`, deregister on `Drop`
3. `track_read(id)`: panics if write held; increments read count
4. `track_write(id)`: panics if any lock held; sets write flag
5. `untrack_read/write(id)`: decrements/clears

Zero-cost in release (`cfg(debug_assertions)`).

## Files touched (4)
- `crates/core/src/ecs/lock_tracker.rs` (NEW)
- `crates/core/src/ecs/query.rs` (add TypeId + Drop)
- `crates/core/src/ecs/resource.rs` (add TypeId + Drop)
- `crates/core/src/ecs/world.rs` (pass TypeId to guard constructors)
