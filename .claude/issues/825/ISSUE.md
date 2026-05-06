ECS-PERF-03: transform_propagation_system rediscovers root entities every frame by scanning every Transform|## Description

Phase 1 of the propagation system iterates every entity in the `Transform` storage and calls `parent_q.get(entity).is_none()` to identify roots. The root set is *nearly static* — it changes only on top-level spawn/despawn (cell load, cell unload, NPC spawn).

On Megaton baseline the Transform storage holds ~6 000 entities (every NIF subnode gets one); the per-frame scan is therefore ~6 000 iterator steps + 6 000 `Parent` storage lookups, every frame, in steady state inside a static interior.

The same scratch (`roots: Vec<EntityId>`) is captured across frames and cleared, so the allocation is amortized — but the work to *fill it* is not.

## Location

`crates/core/src/ecs/systems.rs:72-81`

## Evidence

```rust
// Phase 1: find root entities (have Transform but no Parent).
for (entity, _) in tq.iter() {
    let is_root = parent_q
        .as_ref()
        .map(|pq| pq.get(entity).is_none())
        .unwrap_or(true);
    if is_root {
        roots.push(entity);
    }
}
```

## Impact

At Megaton steady-state (~6 000 Transforms, ~30 roots), the scan does ~5 970 wasted lookups per frame. Each `parent_q.get` is a sparse-set hash + bounds check, ~30–50 ns; total ~250 µs/frame for nothing.

At 60 fps that's 1.5 % of the frame budget burned to confirm a list that hasn't changed since cell load. Scales linearly with content — a populated FNV exterior grid (radius 3) can push Transform counts to ~30 000 with the same handful of roots; the wasted scan grows to ~1.5 ms/frame.

## Related

- ECS-PERF-04 (same pattern in `world_bound_propagation_system` — same fix unifies both)
- #791 (E-N2: `unload_cell` victim collection — same family of "scan every X to find a small subset" anti-pattern)

## Suggested Fix

Maintain a `RootEntities: Resource<HashSet<EntityId>>` updated by the spawn/despawn path: insert when an entity gets a Transform without a Parent, remove when it gets a Parent or is despawned. The propagation system reads the set in O(roots) instead of O(transforms). Initial population happens at cell-load time (~one walk over the freshly-loaded subtree).

Alternative cheaper interim: compare `Transform::len()` and `Parent::len()` against last-frame values; only re-scan if either changed (matches the `NameIndex.generation` pattern already used elsewhere).

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: ECS-PERF-04 must be fixed in the same PR — same fix
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: New `RootEntities` resource — pick TypeId-stable name; verify schedule doesn't cycle a write on it during frame
- [ ] **FFI**: N/A
- [ ] **TESTS**: Add a benchmark that measures propagation cost vs. transform count with a fixed root count; also add a unit test for the spawn/despawn maintenance hooks

## Source Audit

`docs/audits/AUDIT_PERFORMANCE_2026-05-04_DIM4.md` — ECS-PERF-03