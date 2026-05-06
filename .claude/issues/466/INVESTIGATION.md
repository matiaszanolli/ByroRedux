# Investigation — #466

**Domain**: ecs

## Code path

`World::despawn` (`crates/core/src/ecs/world.rs:108-119`) iterates `self.storages` (a `HashMap<TypeId, RwLock<Box<dyn DynStorage>>>`) and `get_mut()` on the lock to remove the entity from each storage. On lock poisoning, it calls `storage_lock_poisoned_erased("<unknown>")` — losing the type name even though `type_id` is in scope.

Other poison call sites (`storage_lock_poisoned::<T>()`) have a generic `T` and recover the name via `std::any::type_name::<T>()`. `despawn` runs over a type-erased map, so it has no `T` — only a `TypeId`. `TypeId` doesn't expose the source type name.

## Storage creation sites (where to record names)

Two sites, both `entry().or_insert_with(...)`:
- `World::register<T>()` (L93)
- `World::storage_write<T>()` (L711) — called by `insert` and `insert_batch`

Both have `T: Component` in scope, so `std::any::type_name::<T>()` is available.

## Fix

Add `type_names: HashMap<TypeId, &'static str>` side-table to `World`. Populate at both storage-creation sites. `despawn` looks up the name; falls back to `"<unknown>"` for the unreachable case.

Single caller of `storage_lock_poisoned_erased` (verified via grep) — rename or extend in place; no other call sites.

## Scope

1 file: `crates/core/src/ecs/world.rs`
