# CONC-D5-2026-08-30-03: the canonical order table omits `PhysicsWorld` entirely, so the rule the live cycle breaks is unwritten

**Issue**: #3656
**Labels**: documentation, ecs, low, concurrency, doc-rot
**Filed**: 2026-08-30
**Source report**: `docs/audits/AUDIT_CONCURRENCY_2026-08-30.md`

---

Source: `docs/audits/AUDIT_CONCURRENCY_2026-08-30.md` — CONC-D5-2026-08-30-03 (LOW, D5 · RwLock Patterns — doc gap).

**Location**: `docs/engine/ecs.md:596-635`.

## Description

The "Canonical acquisition order" block names the hierarchy/skinning/bounds cluster —

```
CharacterController → RapierHandles → Transform → Parent → Children → GlobalTransform
                    → SkinnedMesh → MeshHandle → LocalBound → WorldBound → Name → StringPool
```

— and gives the CHARAL pair (`CharacterRuleset -> ActorValues`) its own paragraph, but **says nothing about `PhysicsWorld`**, even though it is the single most widely co-acquired resource in the engine, participates in the cluster through `RapierHandles`/`GlobalTransform`, and is now the hub of a live cycle (CONC-D5-2026-08-30-01).

The actual rule — *"`PhysicsWorld` is acquired last and no storage may be acquired while it is held"* — exists only as prose inside individual functions (`crates/physics/src/sync.rs:236-246`, `:611-614`, `byroredux/src/scene.rs:232-235`), **one of which (`byroredux/src/commands/view.rs:184-185`) states it while doing the opposite**.

## Evidence

`docs/engine/ecs.md:601-604` (the code block) contains **no resource type at all**; the paragraph at `:620-628` covers only `CharacterRuleset` / `ActorValues` / `CharacterLevel`. Verified by grep: `PhysicsWorld` appears nowhere in `docs/engine/ecs.md`.

## Impact

**A reviewer following the documented order has no signal that `byroredux/src/commands/view.rs:175` is wrong.** The rule that the sweep's only live lock cycle breaks is unwritten.

## Related

#2404, #3441, #313; **CONC-D5-2026-08-30-01** (the live cycle this omission permitted), **CONC-D3-2026-08-30-04** (the same table gap seen from the ECS dimension — three more undocumented clusters).

## Suggested Fix

Add `PhysicsWorld` as an explicit tail of the physics prelude in the canonical table — *"storages first, `PhysicsWorld` last, nothing acquired under it"* — and name the `collect_newcomers` / `ragdoll_writeback_system` sites as the worked examples, the way the `CharacterRuleset` paragraph names `pool_regen_tick_system`.

## Completeness Checks
- [ ] **SIBLING**: Fold in CONC-D3-2026-08-30-04's three clusters in the same edit — the two findings are one table gap seen from two dimensions
- [ ] **LOCK_ORDER**: The worked examples cited must be sites that actually follow the rule; `byroredux/src/commands/view.rs` must not be cited until CONC-D5-2026-08-30-01 is fixed
- [ ] **TESTS**: Consider whether the lock tracker can dump its observed graph so the table can be diffed against reality
