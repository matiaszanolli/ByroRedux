# Investigation — #883 (CELL-PERF-06)

## Domain
ecs (cell unload + ECS storage queries)

## Hot path

`unload_cell` (`byroredux/src/cell_loader.rs:132`) currently runs six
sequential `for &eid in &victims` loops, one per per-component query.
Each loop:
  1. takes a `world.query::<T>()` read lock on T's SparseSet
  2. walks the entire victim list looking up `T` per entity
  3. drops the lock

Components covered: `MeshHandle`, `TextureHandle`, `NormalMapHandle`,
`DarkMapHandle`, `ExtraTextureMaps`, `TerrainTileSlot`.

## Why the fix is safe

`world.query::<T>()` (`crates/core/src/ecs/world.rs:349`) takes a read
lock on T's SparseSet. Six separate read locks on six DIFFERENT
component types don't deadlock — they're independent locks.
`unload_cell` takes `&mut World` so no concurrent writer to any of
these SparseSets can exist (engine is single-threaded for cell-unload
today).

The TypeId-sorted-acquisition invariant (CLAUDE.md #4) applies to
multi-component combined queries (`query_2_mut` etc) where mixing
read+write across N types could deadlock with another thread taking
the same N locks in a different order. Six READ locks have no such
risk; we're free to acquire them in source order.

## Refactor

Hoist all six query handles before the victim loop, then do a single
walk that fans out to all six lookups:

```rust
let mq = world.query::<MeshHandle>();
let tq = world.query::<TextureHandle>();
let nq = world.query::<NormalMapHandle>();
let dq = world.query::<DarkMapHandle>();
let eq = world.query::<ExtraTextureMaps>();
let ttq = world.query::<TerrainTileSlot>();

for &eid in &victims {
    if let Some(mq) = &mq {
        if let Some(mh) = mq.get(eid) { mesh_drops.push(mh.0); }
    }
    if let Some(tq) = &tq { /* … */ }
    /* … */
}
```

Eliminates 5 redundant SparseSet header walks. Per-victim inner cost
is unchanged (still 6 hash lookups per victim).

## Files affected

1. `byroredux/src/cell_loader.rs` (single function — `unload_cell`)

Single file, low risk.

## Test approach

`unload_cell` requires a live `VulkanContext` so it can't be
unit-tested in isolation. The existing
`cell_loader_sky_params_cleanup_tests` covers the post-loop resource
cleanup path; the per-component cleanup is exercised by every cell
load/unload in the integration path (`main.rs:544/827`). Behavior is
preserved by construction: same lookups, same destination Vec/HashSet,
same order — only the loop nesting changes.
