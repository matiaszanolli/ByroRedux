# 3267: CONC-D5-2026-08-24-04: physics_sync_system invoked re-entrantly from three non-scheduler sites with no documented exclusivity requirement

**Severity**: LOW · **Report**: `docs/audits/AUDIT_CONCURRENCY_2026-08-24.md` (CONC-D5-2026-08-24-04)

## Description

The full 4-phase physics tick is invoked with `dt = 0.0` from three sites outside `Scheduler::run`, purely to register newcomers so the query pipeline can be flushed for a floor probe. Nothing states these entries require an exclusive/`&mut World` context; `physics_sync_system` takes `&World`, so the type system doesn't enforce it. Safe today only because all three call sites happen to run outside the scheduler or inside an exclusive lane.

## Location

`byroredux/src/systems/character.rs:772`, `byroredux/src/commands/view.rs:160`, `byroredux/src/scene.rs:1163`

## Impact

No defect today, but the exercised write surface (`Transform`, `RapierHandles`, `WaterContact`, `PhysicsWorld`, `WaterContactScratch`) is invisible to the access analyzer from these call paths, and one sits on the un-owned debug console command surface. A future console command or scheduler registration reaching one of these helpers inherits an undeclared full physics tick.

## Related

#3130 (CONC-D5-2026-08-24-01); cross-referenced ECS-2026-08-24-05/#3253 (same declaration's `Parent`/`ActorBoneCollider` gap).

## Suggested Fix

Document the requirement on `physics_sync_system` (exclusive/`&mut World` context only), or extract the narrower "register newcomers, flush pipeline" operation into its own helper.

## Completeness Checks
- [ ] **TESTS**: N/A — documentation/extraction fix
