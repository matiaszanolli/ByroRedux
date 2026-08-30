# CONC-D3-2026-08-30-04: the canonical order table covers 3 clusters; the animation, scene/quest and physics-shape clusters have no documented direction

**Issue**: #3651
**Labels**: documentation, ecs, low, concurrency, doc-rot
**Filed**: 2026-08-30
**Source report**: `docs/audits/AUDIT_CONCURRENCY_2026-08-30.md`

---

Source: `docs/audits/AUDIT_CONCURRENCY_2026-08-30.md` — CONC-D3-2026-08-30-04 (LOW, D3 · ECS Lock Ordering & Deadlock — doc gap / **root cause**).

**Location**: `docs/engine/ecs.md:598-636`.

## Description

The canonical acquisition-order table gives one total order for `CharacterController -> ... -> Name -> StringPool` plus the `CharacterRuleset -> ActorValues` pair (#3441). It has **no entry** for three clusters that live systems demonstrably hold together:

**(a)** `AnimationClipRegistry` / `NameIndex` / `AnimationPlayer` — the #2400 "two outermost locks" invariant exists only as a comment at `byroredux/src/systems/animation.rs:527-529` and `:598-600`.

**(b)** `ScenePlayer` / `SceneRegistry` / `QuestStageState` / `QuestAdvanceOnActivate` — no documented direction anywhere.

**(c)** `CollisionShape` / `RigidBodyData` / `ActorBoneCollider` / `PhysicsWorld` — documented only as a local comment at `crates/physics/src/sync.rs:840-842`.

**CONC-D3-2026-08-30-02 and -03 are direct consequences of (a) and (b)**: a second crate reconstructed an order from scratch and got the opposite one. This is the same shape as the 2026-08-24 finding that the table omitted `CharacterController`/`RapierHandles`, which was fixed by adding them.

## Evidence

```
docs/engine/ecs.md:600-604
CharacterController → RapierHandles → Transform → Parent → Children
                    → GlobalTransform → SkinnedMesh → MeshHandle
                    → LocalBound → WorldBound → Name → StringPool
```

Verified by grep: no `AnimationClipRegistry`, `NameIndex`, `AnimationPlayer`, `ScenePlayer`, `SceneRegistry`, `CollisionShape`, `RigidBodyData`, `ActorBoneCollider` or `PhysicsWorld` appears anywhere in that section.

## Impact

The knowledge exists in three local comments; **the arbiter the audit checklist points at does not carry it.** Every new consumer of those types re-derives an order by guesswork.

This is the cross-cutting root cause behind this sweep's lock-order findings: **fixing the table is cheaper than fixing them one at a time.** One of the local comments (`byroredux/src/commands/view.rs:184-185`, see the HIGH finding) *states the rule while doing the opposite*.

## Related

#2400, #2388, CONC-D3-2026-08-30-02, CONC-D3-2026-08-30-03, and CONC-D5-2026-08-30-03 (the `PhysicsWorld` omission, the same defect from the physics side).

## Suggested Fix

Add the three clusters to the canonical table with the direction the per-frame system already establishes, citing the establishing site for each as the existing entries do:

- `AnimationClipRegistry -> NameIndex -> AnimationPlayer`
- `ScenePlayer -> SceneRegistry`
- `RapierHandles -> CollisionShape -> RigidBodyData -> GlobalTransform -> ActorBoneCollider`

## Completeness Checks
- [ ] **SIBLING**: Fold in CONC-D5-2026-08-30-03's `PhysicsWorld` tail in the same edit — the two findings are one table gap seen from two dimensions
- [ ] **LOCK_ORDER**: Each added direction is the one the *per-frame* system establishes, verified against the establishing site, not inferred
- [ ] **TESTS**: Consider whether the lock tracker can emit its observed graph so the table can be diffed against reality rather than hand-maintained
