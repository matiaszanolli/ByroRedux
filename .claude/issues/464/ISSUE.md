# Issue #464

E-01: Transform propagation is DFS (queue.pop is LIFO) but comments + variable name say BFS

---

## Severity: Low (docs/naming)

**Location**: `crates/core/src/ecs/systems.rs:92,109`

## Problem

`make_transform_propagation_system` is documented as "breadth-first walk" (comment at line 27, 48, 92) and the traversal buffer is named `queue`. The implementation at line 109 calls `queue.pop()`, which is `Vec::pop` → LIFO. That's **DFS**, not BFS.

The walk is **functionally correct** — each child's parent GlobalTransform is written in phase 1b (or earlier in the same DFS branch) before the child pops — so this is a naming/docs hygiene issue, not a correctness bug.

## Impact

Anyone reading the code and assuming level-order traversal for a future optimization (e.g. parallel per-level dispatch, depth-aware budget splitting) will get a surprise. No runtime bug.

Distinct from #46 (lock-acquisition perf) — that issue uses "BFS" in the title but tracks a different concern.

## Fix

**Preferred — (a)**: Switch to `VecDeque<EntityId>` + `pop_front` for true BFS. Near-zero cost, docs stay honest, future parallel per-level dispatch becomes trivial.

**Alternative — (b)**: Update comments to say "DFS walk" and rename `queue` → `stack`. Keeps current behavior but closes the door on cheap per-level parallelization.

## Completeness Checks

- [ ] **TESTS**: Existing `transform_propagation_*` tests should pass unchanged (behavior is equivalent for correctness)
- [ ] **DOCS**: Comments at lines 27, 48, 92 updated or the implementation switched
- [ ] **SIBLING**: Check other scene-graph traversals (e.g. `Children` queries) for the same doc/impl mismatch
- [ ] **LOCK_ORDER**: If the buffer type changes, verify no new locks introduced

Audit: `docs/audits/AUDIT_ECS_2026-04-19.md` (E-01)
