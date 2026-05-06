ECS-PERF-04: world_bound_propagation_system rediscovers roots every frame by scanning every GlobalTransform|## Description

Identical anti-pattern to #825 (ECS-PERF-03), but iterates `GlobalTransform` instead of `Transform`. Both storages have the same population (one entry per ECS entity that participates in the scene graph), so the per-frame cost is the same.

This system also runs second in the PostUpdate stage right after `transform_propagation_system`, so the wasted work is **doubled** in practice (two systems both scanning the full transform storage to find the same root set).

## Location

`byroredux/src/systems.rs:921-935`

## Evidence

```rust
let Some(tq) = world.query::<GlobalTransform>() else { return; };
let parent_q = world.query::<Parent>();
for (entity, _) in tq.iter() {
    let is_root = parent_q
        .as_ref()
        .map(|pq| pq.get(entity).is_none())
        .unwrap_or(true);
    if is_root { roots.push(entity); }
}
```

## Impact

Same as ECS-PERF-03 — ~250 µs/frame wasted at Megaton steady-state, scaling linearly with scene-graph population. Combined with #825, the two systems waste ~500 µs/frame doing the same root-discovery work twice.

## Related

- #825 (ECS-PERF-03 — same fix unifies both call sites)
- #791 (same family)

## Suggested Fix

Same as #825; both systems consume the same maintained `RootEntities` resource. If a `RootEntities` resource is a heavier change than warranted, a cheaper interim is to cache the root set inside the system closure (already a `move` closure with captured `roots: Vec<EntityId>`) and invalidate when `Transform::len() != last_seen_len` — same generation pattern as `NameIndex` and `SubtreeCache`.

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Fix #825 in the same PR — both systems share the same root set
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: If sharing `RootEntities` resource with #825, both systems read it in PostUpdate — verify no writer interleaves
- [ ] **FFI**: N/A
- [ ] **TESTS**: Same benchmark as #825 covers both

## Source Audit

`docs/audits/AUDIT_PERFORMANCE_2026-05-04_DIM4.md` — ECS-PERF-04