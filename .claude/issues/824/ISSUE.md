ECS-PERF-02: animation_system rebuilds NameIndex HashMap from scratch on every Name-count change|## Description

When the count of `Name` components differs from `NameIndex.generation` (i.e. any Name was added or removed), the system allocates a fresh `std::collections::HashMap::new()`, walks every `Name` entity, inserts into the new map, then swaps it in.

With cell streaming actively touching Name components every frame during a transition (megaton/wasteland: hundreds to thousands of Names), this fires the rebuild repeatedly and thrashes the allocator. The existing comment claims "only when count changes," but cell streaming and any spawn/despawn under `Name` invalidates that condition every frame during the transition.

## Location

`byroredux/src/systems.rs:323-343` (inside `animation_system`)

## Evidence

```rust
if needs_rebuild {
    let name_query = match world.query::<Name>() { ... };
    let mut new_map = std::collections::HashMap::new();   // fresh alloc
    for (entity, name_comp) in name_query.iter() {
        new_map.insert(name_comp.0, entity);              // grow + rehash
    }
    drop(name_query);
    let mut idx = world.resource_mut::<NameIndex>();
    idx.map = new_map;                                    // drop old map
    idx.generation = current_name_count;
}
```

## Impact

For a fully populated cell (~1500 Names per Megaton baseline), each rebuild allocates one HashMap with ~16 incremental rehashes (capacity doubling 0→1→...→2048) plus 1500 entries. ~50–100 µs per rebuild.

During an exterior-cell stream-in event multiple cells settle their Names over consecutive frames, so this fires every frame for ~30 frames (~3 ms total). Steady-state interior: zero rebuilds — the check correctly stabilizes. The cost is concentrated at cell transitions where the user already feels a hitch.

## Suggested Fix

Reuse the existing `NameIndex.map` instead of allocating:

```rust
let mut idx = world.resource_mut::<NameIndex>();
idx.map.clear();
for (entity, name_comp) in name_query.iter() {
    idx.map.insert(name_comp.0, entity);
}
idx.generation = current_name_count;
```

The HashMap retains its allocated buckets across rebuilds, eliminating ~30 µs/rebuild and the per-frame churn during cell streaming. Bonus: pre-size with `idx.map.reserve(current_name_count.saturating_sub(idx.map.len()))` if the count is growing.

Note the lock-order change — `name_query` (read on Name) and `idx` (write on NameIndex resource) need to be live simultaneously. They target different storages (component vs resource), so no conflict; but verify the resource lock's TypeId order matches existing patterns.

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Check `SubtreeCache` rebuild at `systems.rs:311-315` — it already uses `cache.map.clear()`, the right pattern. No sibling fix needed.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: Verify Name read query and NameIndex resource write don't conflict (different TypeIds, should be fine)
- [ ] **FFI**: N/A
- [ ] **TESTS**: Add a regression test that holds the `NameIndex.map`'s allocation address stable across N rebuilds

## Source Audit

`docs/audits/AUDIT_PERFORMANCE_2026-05-04_DIM4.md` — ECS-PERF-02