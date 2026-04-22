# #523 Investigation

## Claim verified

`cell_loader.rs:1205-1239` inside the per-REFR loop at `:1081-1254`. Every iteration performs:

```rust
let mut reg = world.resource_mut::<NifImportRegistry>();  // ← write lock
let hit = reg.cache.get(&cache_key).cloned();
if let Some(entry) = hit {
    reg.hits += 1;
    entry
} else {
    drop(reg);
    // parse outside lock...
    let mut reg = world.resource_mut::<NifImportRegistry>();  // ← write lock again
    reg.misses += 1;
    // ...
    reg.cache.insert(cache_key, parsed.clone());
}
```

Each `resource_mut` is a `RwLock::write` — an atomic CAS + potentially an unpark-all. For 809 REFRs that's 809 CAS round-trips on the hot path.

## LOCK_ORDER

Only `NifImportRegistry` is accessed as a resource inside the loop. Cross-borrow with `&mut World` on `spawn_placed_instances` is already handled by dropping the guard between iterations. No other resources → no LOCK_ORDER concern.

## SIBLING

`load_exterior_cells` (byroredux/src/cell_loader.rs:376) also calls `load_references` (line 551). A single fix covers both paths.

## Plan

Batch the registry access:

1. Outside the loop, declare:
   - `this_call_hits: u64`, `this_call_misses: u64`, `this_call_parsed: u64`, `this_call_failed: u64`
   - `pending_new: HashMap<String, Option<Arc<CachedNifImport>>>`

2. In the loop, per key:
   - Check `pending_new` first (this call's own parses; zero lock cost).
   - On miss, read-lock registry (`resource::<>()`) once, clone the entry. Read locks are cheap (no CAS serialization between readers).
   - On registry miss, parse outside any lock, insert into `pending_new`.

3. After the loop, one `resource_mut` write lock to commit counter deltas + merge `pending_new` into `reg.cache`. End-of-cell stats snapshot happens in the same write-lock scope.

Net: 809 write locks → 1 write lock + ~500 read locks per cell (one per unique mesh path).
