# PERF-D7-02: tag_descendants_as_actor re-walks the whole attached subtree from scratch after every NPC part attach

Filed from: `docs/audits/AUDIT_PERFORMANCE_2026-08-03.md`
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2276
Labels: medium, performance, bug

**Severity**: MEDIUM
**Dimension**: World Streaming & Cell Transitions (7) / NPC Spawn

**Location**: `byroredux/src/npc_spawn/resumable.rs:1126-1133` (`parent_part`, new in `9bf4c493`), plus the `Finalize` call at `:782`/`:1092`; walked function at `byroredux/src/npc_spawn.rs:856-885` (`tag_descendants_as_actor`)

## Description
Pre-`9bf4c493`, `tag_descendants_as_actor` ran exactly once, at the end of each NPC's spawn. The resumable rewrite now calls it after **every** skeleton/body-piece/head/hair/brow/eye/armor attach (8 call sites of `parent_part(...)` in `resumable.rs` at lines 553, 585, 719, 869, 907, 986, 1027, 1051 — several inside per-piece loops, so roughly 11-16 actual invocations for a typical FaceGen actor with body + head + hair/brow + eyes + several armor pieces), plus once more at `Finalize`. Each call does a fresh BFS from the actor root over every currently-attached descendant — two fresh ECS queries and a freshly-allocated `Vec` queue — re-tagging entities a previous call already tagged. This runs on every streamed NPC now, not just occasionally.

## Evidence
`parent_part` (resumable.rs:1126-1133):
```rust
fn parent_part(world: &mut World, placement_root: EntityId, part_root: EntityId) {
    world.insert(part_root, Parent(placement_root));
    add_child(world, placement_root, part_root);
    tag_descendants_as_actor(world, placement_root);
}
```
called from 8 distinct sites in `resumable.rs`, in addition to the 3 direct `tag_descendants_as_actor(world, state.placement_root)` calls at lines 782/1092/1132.

## Impact
Real, avoidable multiplier on the streaming hot path: roughly quadratic in part count per actor (bounded — actor subtrees are small, ~10-20 entities — so not catastrophic, but it is wasted CPU reintroduced by this rewrite that did not exist before it).

## Suggested Fix
Have each `parent_part` call site tag only the specific entity/subtree it just attached (the caller already knows exactly what was added), or defer all tagging to the single `Finalize` call.

## Completeness Checks
- [ ] **SIBLING**: Confirm no other resumable-assembly call site (e.g. equipment re-attach at runtime) has the same re-walk-from-scratch pattern
- [ ] **TESTS**: A regression test pins this specific fix (e.g. asserting `tag_descendants_as_actor`/equivalent runs O(1) times per attach, not re-walking the whole subtree)
