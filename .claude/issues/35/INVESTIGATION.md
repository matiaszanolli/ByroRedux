# Investigation: Issue #35

## Root Cause
World::get<T>(&self) at world.rs:73-85:
1. Acquires RwLockReadGuard on the storage
2. Gets a raw pointer to the component via storage.get()
3. Guard is dropped at end of function scope
4. Returns the pointer as &T tied to lifetime of &self

After the guard drops, query_mut::<T>() can acquire a write lock (since
it also takes &self), and mutate/remove the component — invalidating
the reference. This is unsound.

## Sibling Check
- get_mut() takes &mut self and bypasses the RwLock via .get_mut() — sound
  (exclusive access guaranteed by &mut self)
- No other unsafe blocks in the ECS module

## Fix
Create ComponentRef<'w, T> that owns the RwLockReadGuard and holds a
pointer to the specific component. Deref to &T. Same pattern as
ResourceRead<R> which already works correctly.

All callers use .unwrap().field or .unwrap().0 immediately — auto-deref
through ComponentRef will make this transparent.

## Scope
1 file (world.rs): change get() return type, add ComponentRef struct.
~30 callers use auto-deref so no code changes needed at call sites.
