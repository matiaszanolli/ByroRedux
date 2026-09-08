# Issue #4062 — ECS-2026-09-08-D8-03: the transform-propagation BFS marks every parent dirty in GlobalTransform just to read it

Filed: 2026-09-08 from `docs/audits/AUDIT_ECS_2026-09-08.md` via `/audit-publish`
Repo state at filing: `bb8ced68`
Labels: low, ecs, performance, bug

---

**Severity**: LOW · **Dimension**: 8 (hot-path)
**Location**: `crates/core/src/ecs/systems.rs` — `let Some(parent_global) = gq.get_mut(parent_id).map(|g| *g)` in the BFS drain, and `gq.get_mut(parent.0).map(|g| *g)` in the incremental seeding branch
**Source**: `docs/audits/AUDIT_ECS_2026-09-08.md`

## Description

Both sites are pure reads — the comment says so ("the deref copies it out") — but `PackedStorage::get_mut` calls `mark_dirty` unconditionally, and `GlobalTransform` is `TRACK_CHANGES = true`. Every BFS step therefore pushes its **parent** into `GlobalTransform`'s dirty set, on top of the legitimate mark for the child it actually writes.

`QueryWrite::get` is the non-marking accessor and is already in the API (`crates/core/src/ecs/query.rs`).

## Evidence

A 3-wide, 4-deep tree (40 entities) after one structural walk:

```
entities=40  GlobalTransform dirty entries after one structural walk = 119  (unique 40)
```

40 are the inserts, 40 the real writes, 39 the spurious parent reads.

## Impact

Correctness-neutral — re-folding an unmoved bound is idempotent. But it is ~1 extra `EntityId` push per non-root entity per walked frame, and `bounds.rs` then pays for it twice: a larger `sort_unstable + dedup` over `g_dirty`, and a Pass-1 `WorldBound` recompute plus a Pass-2 root climb for parents whose global did not change.

On the incremental path — the one the whole change-detection design exists to keep small — a single moved leaf dirties its parent too, so the "touch ~1 subtree" claim is really "~1 subtree plus its ancestors' bounds".

## Related

- Same function as ECS-2026-09-08-D8-01 / -D8-02; independent of both.
- `bounds.rs` is `GlobalTransform`'s sole dirty-set drainer, so it absorbs the entire inflation.

## Suggested Fix

`gq.get(parent_id).copied()` at both sites. `GlobalTransform: Copy`, and the shared borrow ends before the `get_mut` write that follows, so it is a drop-in.

## Completeness Checks
- [ ] **SIBLING**: sweep for other read-only `get_mut` on a `TRACK_CHANGES` storage (`Transform`, `GlobalTransform`) — `byroredux/src/systems/billboard.rs` already carries a comment about this hazard
- [ ] **TESTS**: a regression test asserts a read-only parent lookup does not enter `GlobalTransform`'s dirty set (e.g. dirty-entry count equals the number of entities actually written)
