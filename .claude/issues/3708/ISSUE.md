# #3708 — ECS-2026-08-30-P2-03: the alive→dead transition never retires the actor from the ambient-package scheduler, so every corpse pays a heap clone per frame forever

*Filed 2026-08-30 from `docs/audits/`. Immutable snapshot of the issue as filed (TD10-001 / #1156); GitHub is authoritative for current state.*

**Severity**: MEDIUM · **Dimension**: P2 Gameplay Slice / Component Lifecycles
**Location**: `byroredux/src/combat.rs` (`reconcile_dead_actor`, ~:420-433) + `byroredux/src/npc_spawn/ai_package.rs` (pass-2 gate ~:596-628, pass-3 clone + `Dead` skip ~:611-626)
**Source**: `docs/audits/AUDIT_ECS_2026-08-30.md` (ECS-P2-03, `[P2-gameplay]`)

> **Coverage note**: this file has no owner audit skill. The finding comes from the `/audit-ecs` run's explicit P2-gameplay slice sweep and is the only audit coverage it received.

**Status note**: residual of CLOSED #3353, **not** a regression — the once-per-game-minute gate still works for live actors. Dead actors were left uncovered.

## Description

`reconcile_dead_actor` leaves `AmbientPackageRuntime` on the corpse. The pass-2 gate is `requested.contains(actor) || *last != Some(minute)`, and `last_evaluated_game_minute` is written only in the trailing loop over `updates` — which dead actors never reach, because the `Dead` skip `continue`s *after* pass 3 has already cloned the runtime. A corpse's minute marker therefore freezes at its final live evaluation and it is "due" on every frame whose game-minute differs, forever.

## Evidence

```rust
// byroredux/src/npc_spawn/ai_package.rs — the gate; no Dead filter
let due: Vec<EntityId> = last_evaluated
    .into_iter()
    .filter(|(actor, last)| requested.contains(actor) || *last != Some(minute))
```

```rust
// byroredux/src/npc_spawn/ai_package.rs — pass 3 clones FIRST, then skips the dead
    .map(|query| { due.iter().filter_map(|&actor| query.get(actor).map(|r| (actor, r.clone()))).collect() })
// ...
for (actor, runtime) in runtimes {
    if world.get::<Dead>(actor).is_some() {
        continue;                                 // never reaches `updates`
```

`AmbientPackageRuntime` owns `package_candidates: Vec<u32>`, so `r.clone()` is a real allocation.

## Impact

One heap allocation + one `Dead` lookup per corpse per frame, permanently, growing linearly with the session's kill count — exactly the cost model #3353 was closed to remove, with the dead population left uncovered. Freed each frame, so churn rather than a leak: MEDIUM.

## Suggested Fix

Add `remove_component::<AmbientPackageRuntime>(world, actor)` (and `EvaluatePackageRequest`) to the death teardown — a corpse has no package to select. Cheaper alternative: move the `Dead` check into pass 2's `due` filter so the clone never happens.

## Completeness Checks
- [ ] **SIBLING**: The rest of the death-teardown roster checked for other survivors driving per-frame work
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test kills an actor and asserts zero package-runtime clones on subsequent frames
