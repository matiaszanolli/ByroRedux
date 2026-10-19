# CONC-SD-03: `tag_descendants_as_actor` walks `Children` with no visited set and no traversal budget

Labels: low,ecs,tech-debt,bug

**Severity**: LOW (adjacent to the concurrency lens; owner `/audit-ecs` Dim 3). A corrupt/cyclic `Children` graph would spin the main thread with an unbounded `to_tag`; a cycle cannot arise from the spawn path itself (it walks a hierarchy the spawn job just built), so impact needs another bug.
**Dimension**: ECS hierarchy walk (termination, not lock order)
**Location**: `byroredux/src/npc_spawn.rs:1678-1707` (`tag_descendants_as_actor`)

## Description
`let mut queue = vec![root]; while let Some(e) = queue.pop() { ... queue.push(c) for each child ... }` with `children_q` and `mesh_q` guards live. No `seen` set, no budget. #5513 and #5713 (both closed) routed the other whole-subtree walks through `collect_subtree`; the 10-12 ECS report explicitly set aside the "BFS walks that interleave per-node work and lock-held queries" (`ragdoll.rs`, `bounds.rs`), and this one was not named either. Unlike `bounds.rs` (which carries a `HierarchyTraversalGuard`), it has none. The "interleaved per-node work" is only a `MeshHandle` membership test, so it is exactly `collect_subtree` plus a filter (the shape `common_shape_rotation` and `mesh_entities_under` now use).

## Related
#5513, #5713, #5954 (closed), CONC-SD-01 (the sibling function in the same spawn tail).

## Suggested Fix
`collect_subtree(world, root)` first, then take `MeshHandle` and filter, as `common_shape_rotation` does; this also removes the hand-held `Children`+`MeshHandle` pair.

Source: docs/audits/AUDIT_CONCURRENCY_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other per-placement components absent from the eviction row)
- [ ] **TESTS**: A regression test pins this specific fix
