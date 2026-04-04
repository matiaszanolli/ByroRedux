# Investigation: Issue #39

## Root Cause
`remove()` and `get_mut()` both delegate to `storage_write()`, which uses
`entry().or_insert_with()` — creating a default storage if one doesn't exist.

This means calling `world.remove::<Foo>(entity)` on a type that was never
inserted creates an empty storage. Subsequent `world.query::<Foo>()` returns
`Some(empty)` instead of `None`, which is semantically incorrect.

## Affected Methods
- `remove()` at line 57-58: should return None if no storage exists
- `get_mut()` at line 80-81: should return None if no storage exists

## Not Affected
- `insert()` at line 52-53: correctly creates storage (it's adding data)
- `storage_write()` itself is fine — it's correct for `insert()`, just
  shouldn't be called from remove/get_mut

## Fix
For both `remove()` and `get_mut()`: check `storages.get_mut()` first,
return `None` if the storage doesn't exist, otherwise downcast and operate.

## Scope
1 file, 2 methods changed. Add regression test.
