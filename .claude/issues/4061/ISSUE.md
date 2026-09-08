# Issue #4061 — ECS-2026-09-08-D8-02: the roots-cache key is blind to a Parent remove/insert pair, and the generations that would catch it are wired only to the fast path

Filed: 2026-09-08 from `docs/audits/AUDIT_ECS_2026-09-08.md` via `/audit-publish`
Repo state at filing: `bb8ced68`
Labels: medium, ecs, bug

---

**Severity**: MEDIUM · **Dimension**: 8 (change detection)
**Location**: `crates/core/src/ecs/systems.rs` — the `roots_key` / `topology_changed` computation and the `if topology_changed { roots.clear(); … }` rebuild in `make_transform_propagation_system`
**Source**: `docs/audits/AUDIT_ECS_2026-09-08.md`

## Description

`topology_changed` — the flag that decides whether to rebuild the cached `roots` set — is keyed on `(Transform::len(), Parent::len(), next_entity_id())` only. `Parent`'s `structural_generation()` *is* read one line earlier, but it feeds `last_state` (the fast-path decision) and never `last_roots_key`.

A frame that removes `Parent` from one entity and inserts `Parent` on another, with no spawn, leaves all three components of the key identical while genuinely changing the root set:

- the detached entity is now a root that is **not** in the stale `roots` list and has no parent to be reached from, so nothing ever recomposes it;
- the newly attached entity is still *in* `roots`, so Phase 1b overwrites its global with its bare local, and `expanded.insert(root)` then makes `enqueue_unique_children` refuse to walk it from its real parent.

This is precisely the class of edit the `structural_gen` counters were added for — the code's own comment says they "catch hierarchy edits (reparent / attach) that move no `Transform` and leave entity counts unchanged" — applied to only one of the two decisions that needs them.

## Evidence

Reproduced with a throwaway integration test at `bb8ced68`. Root `R(+5)` with child `X(+1)`, free-standing `Y(+7)`; then `X` detached, `Y` attached under `R`, `X` moved to `99`:

```
frame1: R=5.0 X=6.0 Y=7.0
frame2: X=6.0 (want 99.0)  Y=7.0 (want 12.0)
frame3: X=6.0 (want 99.0)
```

Both entities are wrong and neither recovers — frame 3 takes the fast path. Note `X`'s `Transform` *was* marked dirty and that did not help: the structural branch ignores `transform_dirty` entirely.

## Impact

Two permanently wrong `GlobalTransform`s per occurrence, same blast radius as ECS-2026-09-08-D8-01 (model matrix, bone palette, `WorldBound`, physics pose).

**Not reachable from any current production call site.** A workspace-wide search finds no `world.remove::<Parent>` outside `byroredux/src/systems/bounds.rs`'s own test module, and `helpers::add_child` only appends to `Children` — it never unlinks a previous parent. A despawn+spawn pairing does not net out either, because `next_entity_id()` advances.

So this is a latent defect whose invariant is upheld by an accident of the current call graph rather than by the key. The first detach path added — item drop, ragdoll limb release, an editor unlink — inherits it silently.

## Related

- ECS-2026-09-08-D8-01 (same function, incremental branch).

## Suggested Fix

Gate the root rebuild on `structural_changed` rather than `topology_changed` — i.e. fold `parent_gen`/`children_gen` into the roots key (`last_roots_key: Option<((usize, usize, EntityId), u64, u64)>`), or simply drop `topology_changed` and rebuild `roots` whenever `structural_changed`.

`topology_changed ⇒ structural_changed` already holds, so this is strictly more conservative and cannot regress the fast path: a reparent-overwrite frame would rebuild `roots` needlessly, which is the same once-per-hierarchy-edit cost the branch already pays.

Add a regression test for the `Parent` remove + insert pair.

## Completeness Checks
- [ ] **SIBLING**: `byroredux/src/systems/bounds.rs` caches `child_roots` on its own structural key — check it is not blind to the same edit
- [ ] **TESTS**: a regression test pins a `Parent` remove + insert in one inter-frame window and asserts both affected entities are correct, including on the following fast-path frame
