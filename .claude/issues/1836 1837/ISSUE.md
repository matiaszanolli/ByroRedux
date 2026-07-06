# #1836 (ECS-2026-07-02-03) + #1837 (ECS-2026-07-02-04) — poison-diagnostics gaps (both LOW)

Two sibling gaps in the #466 loud-poison policy in `crates/core/src/ecs/world.rs`.
Both are post-panic-only diagnostics gaps — no runtime correctness impact — with
clear in-file reference implementations to mirror.

## #1836 — clear_entities dropped the component type name
`clear_entities` (M45 snapshot-restore path) iterated `storages.values_mut()`
and panicked with the generic `"storage lock poisoned during clear_entities"`,
discarding the `TypeId` key it could resolve through the #466 `type_names`
side-table — unlike `despawn`, which names the offending component.

**Fix**: iterate `storages.iter_mut()`, resolve the name via
`type_names.get(type_id)`, call `storage_lock_poisoned_erased(type_name)` —
a 3-line change mirroring `despawn` (world.rs:125-135).

## #1837 — insert_resource swallowed a poisoned prior-value lock
Replacing an existing resource whose lock was poisoned ran
`old.and_then(|lock| lock.into_inner().ok()...)` — `.ok()` maps `PoisonError`
to `None`, indistinguishable from "no prior value existed". `remove_resource`
already re-panics loud (`resource_lock_poisoned::<R>()`).

**Fix**: `old.map(|lock| { lock.into_inner().unwrap_or_else(|_|
resource_lock_poisoned::<R>()) ...expect("resource type mismatch") })` —
mirroring `remove_resource` (world.rs:564-570).

## Sibling sweep
Every other lock-resolution site in world.rs is already name-aware: the typed
`<T>`/`<R>` contexts (insert/query/remove_resource) use the generic
`storage_lock_poisoned::<T>()` / `resource_lock_poisoned::<R>()` (compile-time
type name); the only two type-name-dropping sites were the type-erased
`clear_entities` loop and the `.ok()`-swallowing `insert_resource` — both now
fixed. `despawn` (#466) and `remove_resource` were already correct.

## Tests
- `clear_entities_poisoned_lock_panics_with_type_name` (mirrors
  `despawn_poisoned_lock_panics_with_type_name`): poisons the Health storage,
  asserts `clear_entities` panics naming "Health" — fails pre-fix (generic msg).
- `insert_resource_over_poisoned_lock_panics_with_type_name` (mirrors
  `poisoned_resource_lock_panics_with_type_name`): poisons the ResA lock,
  asserts replacing it panics naming "ResA" — fails pre-fix (silent `None`).

## Domain / verification
ecs → `byroredux-core` (2 files). Scoped suite green (3 poison tests incl. 2
new); full workspace green, no new warnings.
