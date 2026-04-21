# Investigation — Issue #512 (PERF D4-E1)

## Domain
ECS — `crates/core/src/ecs/world.rs`

## Premise verification (per Audit Finding Hygiene memory)

**Audit claim**: "Each insert takes a write lock on the target storage. For a 5000-entity cell load inserting ~5 components per entity, that's **25,000 write-lock acquire/release pairs** during load."

**Actual code path** (`world.rs:126-134`):
```rust
pub fn insert<T: Component>(&mut self, entity: EntityId, component: T) {
    ...
    self.storage_write::<T>().insert(entity, component);
}

fn storage_write<T: Component>(&mut self) -> &mut T::Storage {
    self.storages.entry(TypeId::of::<T>())
        .or_insert_with(|| RwLock::new(Box::new(T::Storage::default())))
        .get_mut()  // <-- RwLock::get_mut is LOCK-FREE with &mut self
        ...
}
```

`RwLock::get_mut()` is lock-free — it returns a direct mutable reference because `&mut self` statically guarantees unique access. No atomic operations, no futex calls. The audit's "lock thrash" framing is wrong.

## Real overhead per insert call

- `self.storages.entry(TypeId::of::<T>())` — HashMap lookup (FxHash, ~10 ns)
- `.or_insert_with(...)` — branch + conditional allocate
- `.get_mut().unwrap_or_else(|_| storage_lock_poisoned)` — poison check
- `.as_any_mut().downcast_mut::<T::Storage>()` — dyn cast + TypeId check
- Then the storage's own insert (O(log n) for Packed binary_search)

On 25,000 calls, even a 100 ns overhead per call totals 2.5 ms — meaningfully less than the audit's 30 ms estimate, but still real. A batch API amortizes the HashMap lookup + downcast across N items for a single type.

## Fix

Add `World::insert_batch<T, I>(items: I)` where `I: IntoIterator<Item = (EntityId, T)>`. Single `storage_write::<T>()` lookup, iterate inserts.

**No call-site migration** in this fix — cell_loader's current shape is "per-entity 3-5 inserts across different types," which batch-insert doesn't directly help without restructuring the loop to first collect all Transforms, then GlobalTransforms, etc. That's a separate refactor with its own profile-evidence requirement.

## Tests

Equivalence: 5000 serial `insert` produces identical storage state to one `insert_batch` with the same items. Pin via query iteration + collected comparison.

## Scope
1 file: `crates/core/src/ecs/world.rs` (add method + test). No API breaking change.
