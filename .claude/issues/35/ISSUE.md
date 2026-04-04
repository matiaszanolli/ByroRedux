# Issue #35: ECS: World::get() unsound — reference outlives RwLockReadGuard

- **State**: OPEN
- **Labels**: bug, ecs, critical, safety
- **Location**: `crates/core/src/ecs/world.rs:65-77`

World::get() acquires RwLockReadGuard, extracts raw pointer, drops guard,
returns pointer as &T. After guard drops, query_mut() can write-lock and
mutate the data, causing use-after-free.

**Fix**: Return a guard-owning ComponentRef<T> that derefs to &T.
