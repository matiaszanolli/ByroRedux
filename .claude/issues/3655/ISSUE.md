# CONC-D5-2026-08-30-02: `ragdoll_writeback_system` acquires `LocalBound`/`WorldBound` under the `PhysicsWorld` guard — a second `PhysicsWorld -> storage` edge

**Issue**: #3655
**Labels**: bug, medium, physics, concurrency
**Filed**: 2026-08-30
**Source report**: `docs/audits/AUDIT_CONCURRENCY_2026-08-30.md`

---

Source: `docs/audits/AUDIT_CONCURRENCY_2026-08-30.md` — CONC-D5-2026-08-30-02 (MEDIUM, D5 · RwLock Patterns — Resource<->Storage & Physics Step).

**Location**: `byroredux/src/ragdoll.rs:494-505`.

## Description

`ragdoll_writeback_system` correctly takes the hierarchy cluster in canonical order and then `PhysicsWorld` last of the *pre-existing* set — but **two more storages are acquired after the resource guard**, inverting the crate-wide "no storage under a `PhysicsWorld` guard" rule.

Unlike the HIGH sibling (CONC-D5-2026-08-30-01) this one has **no opposing edge yet**, so it is a defence-in-depth gap rather than a live defect. It is listed because it is the same shape and **the same file already documents its ordering reasoning**, so the omission reads as intentional.

## Evidence

```
byroredux/src/ragdoll.rs
488	    let transform_q = world.query::<Transform>();
489	    let parent_q = world.query::<Parent>();
490	    let children_q = world.query::<Children>();
491	    let Some(mut gtq) = world.query_mut::<GlobalTransform>() else {
492	        return;
493	    };
494	    let Some(pw) = world.try_resource::<PhysicsWorld>() else {
495	        return;
496	    };
...
504	    let local_bound_q = world.query::<LocalBound>();
505	    let mut world_bound_q = world.query_mut::<WorldBound>();
```

The comment directly above line 488 states the intended discipline (*"GlobalTransform (write) is taken last of the four, then PhysicsWorld last of all — matching `push_kinematic`'s order for that pair (#313)"*), and the #1981 comment between `:496` and `:504` explains the bound queries' grouping — but neither notices that the bounds acquisitions land **under** `pw`.

## Trigger Conditions

**Not cycle-closing today** — nothing establishes `LocalBound => PhysicsWorld` or `WorldBound => PhysicsWorld`. `make_world_bound_propagation_system` (`byroredux/src/systems/bounds.rs:133-172`) walks `Parent -> Children -> GlobalTransform -> SkinnedMesh -> LocalBound -> WorldBound` and never touches `PhysicsWorld`.

It becomes a cycle the moment any bounds-side code path reaches a physics query — e.g. a future `WorldBound`-driven broadphase pre-pass, or a `LocalBound`-keyed collider synthesiser.

## Verification Path

`cargo test` — would surface as a new `lock_tracker` cycle panic naming `PhysicsWorld -> LocalBound` / `PhysicsWorld -> WorldBound` under `BYRO_LOCK_ORDER_CHECK=1`.

## Impact

Widens the `PhysicsWorld -> storage` surface from **one site to two**. A fix for CONC-D5-2026-08-30-01 that does not also cover this leaves the class open — and the class is what turns every one of the many safe `storage -> PhysicsWorld` edges into a cycle.

## Related

#1981 (the pass that added the bound queries), #313, #2388; **CONC-D5-2026-08-30-01** (the live cycle, same class).

## Suggested Fix

Hoist `local_bound_q` / `world_bound_q` **above** the `try_resource::<PhysicsWorld>()` line — they are independent of `pw` — keeping the canonical `... -> GlobalTransform -> LocalBound -> WorldBound` order intact and leaving `PhysicsWorld` as the last acquisition with nothing under it.

## Completeness Checks
- [ ] **LOCK_ORDER**: After the change, `PhysicsWorld` is the final acquisition in the function with no storage taken under it; canonical `GlobalTransform -> LocalBound -> WorldBound` order preserved
- [ ] **SIBLING**: Landed with (or immediately after) CONC-D5-2026-08-30-01 — a fix to one site that leaves the other open does not close the class
- [ ] **TESTS**: `BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux --bins` green, and the ragdoll tests specifically
