# Issue #39: ECS: remove()/get_mut() create empty storage as side effect

- **State**: OPEN
- **Labels**: bug, ecs, low
- **Location**: `crates/core/src/ecs/world.rs:57-59, 80-82`

Both call `storage_write()` which lazily creates empty storage. A no-op
`remove::<Foo>()` permanently changes `query::<Foo>()` from `None` to `Some(empty)`.

**Fix**: Check `storages.get()` first, return `None` early if missing.
