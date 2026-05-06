# Investigation — #824

**Domain**: ecs / animation_system

## Code path

`byroredux/src/systems.rs:323-343` in `animation_system` prelude. When `NameIndex.generation != current_name_count`, the system:

1. Allocates `new_map = HashMap::new()` (zero capacity)
2. Walks every Name entity, inserting into `new_map` (incremental doubling: 0→1→2→...→N, ~log2(N) reallocs)
3. Swaps `idx.map = new_map` — drops the OLD map (with its allocated buckets) and adopts the new

The old map's capacity is wasted on every rebuild. On a populated cell (~1500 Names) each rebuild costs ~16 incremental rehashes plus the bucket-array drop. Cell streaming changes Name counts on consecutive frames, firing the rebuild ~30 times in a row → ~3 ms total spike during transition.

## Fix

`idx.map.clear()` retains the bucket allocation; refill into the same map. `reserve(current_name_count)` guarantees a single rehash up-front rather than growth-doubling, which matters on cold-start when the map is freshly created with capacity 0.

The original code's `drop(name_query); take idx_mut;` ordering was unnecessary — Name (component) and NameIndex (resource) have different TypeIds, no lock-tracker conflict. Combine into one scope: take `idx` write while holding `name_query` read.

## Pre/post

```rust
// Pre — drops the bucket array on every rebuild
let mut new_map = HashMap::new();
for (entity, name_comp) in name_query.iter() { new_map.insert(name_comp.0, entity); }
drop(name_query);
let mut idx = world.resource_mut::<NameIndex>();
idx.map = new_map;

// Post — keeps bucket capacity across rebuilds
let mut idx = world.resource_mut::<NameIndex>();
idx.map.clear();
idx.map.reserve(current_name_count);   // 1 rehash on first fill, no-op afterwards
for (entity, name_comp) in name_query.iter() { idx.map.insert(name_comp.0, entity); }
```

## Scope

1 file: `byroredux/src/systems.rs`. Behavior unchanged. Test strategy: existing animation_system tests cover lookup correctness; allocation reduction requires dhat infra (deferred per the established precedent for this audit batch).
