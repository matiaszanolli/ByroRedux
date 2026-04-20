# Issue #466

E-03: World::despawn poisoned-lock panic loses component type name

---

## Severity: Low (diagnostics)

**Location**: `crates/core/src/ecs/world.rs:108-119`

## Problem

```rust
pub fn despawn(&mut self, entity: EntityId) {
    if entity >= self.next_entity { return; }
    for (type_id, lock) in self.storages.iter_mut() {
        lock.get_mut()
            .unwrap_or_else(|_| storage_lock_poisoned_erased("<unknown>"))
            .remove_entity_erased(entity);
        // `type_id` is present but not used for naming — keep it suppressed.
        let _ = type_id;
    }
}
```

Every other poison path in the file surfaces the concrete type name via `std::any::type_name::<T>()`. Here the `TypeId` is available in the loop variable but deliberately discarded, and the literal `"<unknown>"` is passed to the erased helper.

## Impact

Harder diagnostics when a prior system panic poisons a storage lock AND a subsequent despawn trips the helper. Rare compound failure, but when it happens the panic message is useless.

## Fix

Plumb a `HashMap<TypeId, &'static str>` side-table through `World::register` (populated with `std::any::type_name::<T>()` at registration time). Look up the name in `despawn`. Fallback remains `"<unknown>"` for the never-registered case (which cannot happen given `register` is called from every `insert`, but keep the fallback for defensive safety).

Alternative shorter fix: panic with the `TypeId`'s `Debug` output (e.g. `"TypeId(0x...)"`), which at least lets the user grep for the offending component.

## Completeness Checks

- [ ] **TESTS**: Poison a storage lock, call despawn, assert panic message includes the component's type name
- [ ] **SIBLING**: Audit `storage_lock_poisoned_erased` callers for other `"<unknown>"` passes
- [ ] **LOCK_ORDER**: No lock changes in this fix; no ordering concerns
- [ ] **DOCS**: Document the name-resolution strategy in the `World::register` comment

Audit: `docs/audits/AUDIT_ECS_2026-04-19.md` (E-03)
