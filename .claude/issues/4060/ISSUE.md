# #4060: ECS-2026-09-08-D8-01: a clean node between two dirty entities permanently strands the subtree below the deeper one

Labels: bug, ecs, high

**Severity**: HIGH · **Dimension**: 8 (change detection) — correctness, not perf
**Location**: `crates/core/src/ecs/systems.rs` — the incremental seeding branch of `make_transform_propagation_system`, and `enqueue_unique_children`
**Source**: `docs/audits/AUDIT_ECS_2026-09-08.md`

## Description

On the incremental path, each dirty entity `e` is seeded by composing its global from *its parent's current global* — correct only when no ancestor of `e` is also dirty-but-not-yet-recomposed. The code knows this and relies on the BFS to re-fix `e` later ("the ancestor's seeded subtree re-fixes `e` — order doesn't matter").

That recovery works for `e` itself, but **not for `e`'s children**: seeding `e` enqueues them immediately, they are popped and `expanded.insert`-ed while `e` still holds its wrong global, and when the BFS finally reaches `e` and recomposes it correctly, `enqueue_unique_children` sees them in `expanded` and **refuses to re-enqueue them**, logging `"cyclic or duplicate Children edge"` instead. The subtree below `e` keeps a global composed from `e`'s pre-update value.

It does not self-heal. The next frame nothing is dirty and the state key is unchanged, so the fast path returns immediately and the wrong value survives indefinitely.

The existing regression test `dirty_descendant_is_recomposed_after_a_later_dirty_ancestor` reads as if it covers this, but its fixture is a two-entity chain where the dirty descendant is a **leaf** — precisely the one shape that cannot exhibit the defect.

## Evidence

Reproduced with a throwaway integration test against `make_transform_propagation_system` at `bb8ced68`. Chain `A → B → C → D`, each local `+1x`; `A` and `C` moved, `B` and `D` untouched:

```
frame1: all correct
frame3: idx 3 got 103.0 want 112.0  <<< WRONG
frame8 (nothing moved since): idx 3 got 103.0 want 112.0  <<< WRONG
```

Trace: seed `A` → `A.global = 10`, `expanded = {A}`, enqueue `B`. Seed `C` → parent `B` is still stale (`2`), so `C.global = 102`; enqueue `D`. BFS pops `B` (→ `11`, correct), then `D` (composed from `C = 102` → `103`), then `C` (→ `111`, correct) — and `C`'s re-enqueue of `D` is refused because `D` is already in `expanded`.

Reproduced identically with the entity ids inverted (leaf spawned first). The same fixture with **every** node dirty stays correct, because ascending `EntityId` order then happens to recompose each parent before its child — which is why animation-heavy frames mostly get away with it.

## Impact

A persistently wrong `GlobalTransform` on a whole subtree. `GlobalTransform` is the renderer's model matrix, the skinning palette's bone source, `bounds.rs`'s leaf input, and `physics_sync`'s pose source, so the error shows up as a mis-placed attachment and a mis-sized `WorldBound` (hence wrong culling) until something perturbs the hierarchy.

The trigger is routine content, not a corner case: `animation_system` writes `Transform` only for the channels a clip actually carries, plus the accum root, so any clip that keys the accum root and a distal bone but not every node between them — the normal shape of a `NiControllerSequence`, which carries one `ControlledBlock` per *animated* node — strands whatever hangs off that distal bone. An equipped weapon under a hand bone is the obvious case.

The bogus `"cyclic or duplicate Children edge"` error also mis-attributes an engine defect to the content, once per affected frame.

## Related

- The `expanded` guard was added for #3700 (bounded cyclic traversal); this is that guard firing on a legitimate revisit.
- Sibling defect in the same function's structural branch: ECS-2026-09-08-D8-02.
- #3960 is a *different* `GlobalTransform`-freshness defect (out-of-schedule physics bootstrap), not this one.

## Suggested Fix

Seed only the *shallowest* dirty entities. Build an `FxHashSet` from `transform_dirty` and, for each `e`, climb `Parent` (under the existing `HierarchyTraversalGuard`) — if any ancestor is in the set, skip seeding `e` entirely and let the BFS from that ancestor reach it. That makes every subtree walk top-down exactly once, removes the stale-parent compose, and drops the redundant seed work as a side effect.

Extend `dirty_descendant_is_recomposed_after_a_later_dirty_ancestor` to a four-deep chain with a clean node between the two dirty ones, so the fixture actually exercises what its name claims.

## Completeness Checks
- [ ] **SIBLING**: same seeding pattern checked in `byroredux/src/systems/bounds.rs` (its Pass-2 dirty-root climb walks the same hierarchy)
- [ ] **LOCK_ORDER**: the `Transform → Parent → Children → GlobalTransform` acquisition order is preserved (a `Parent` climb inside the seeding loop reuses the guard already held)
- [ ] **TESTS**: a regression test pins the four-deep chain with a clean node between two dirty entities, and asserts the value survives the next fast-path frame

