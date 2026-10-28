# ECS-2026-10-27-02: vats_scan takes a dozen storages and resources under the GlobalTransform guard

Labels: high, ecs, concurrency, combat, bug
Source: docs/audits/AUDIT_ECS_2026-10-27.md

**Source**: `docs/audits/AUDIT_ECS_2026-10-27.md` finding `ECS-2026-10-27-02` (Dimension 1, lock ordering) · Severity HIGH · Status NEW (introduced by 7eb4de261, Refs #6350; reached from `vats_mode_system`, an Early exclusive, and the `vats.scan` / `vats.queue` console commands).

**Location**: `byroredux/src/systems/vats.rs:339-427` (the `actors` guard and loop), plus `bone_position` (`:436-444`) and `visible_fraction` (`:450-491`), both called inside the loop.

## Description
`let Some(actors) = world.query::<GlobalTransform>()` stays alive for the whole loop body. Under it the loop takes `ActorVitals`, `FormIdComponent`, `Dead`, `CreatureActor` (`world.get`), the `FormIdPool` resource, and `StringPool` + `SubtreeCache` + a second `GlobalTransform` read (`bone_position`). It also takes `PhysicsWorld`, `RapierHandles` and `ActorColliderOwner` (`visible_fraction`). The edge `GlobalTransform -> ActorVitals` closes the existing `ActorVitals -> RapierHandles -> GlobalTransform` chain. The `RapierHandles` read under `GlobalTransform` adds a second inversion edge on top of that.

## Evidence
```rust
let Some(actors) = world.query::<GlobalTransform>() else { return; };
for (entity, transform) in actors.iter() {
    if world.get::<ActorVitals>(entity).is_none() { continue; }
    let Some(form) = world.get::<FormIdComponent>(entity) else { continue; };
    if entity == player || world.get::<Dead>(entity).is_some() { continue; }
    ...
    let global_form = world.try_resource::<byroredux_core::form_id::FormIdPool>()...
```
The detector panics in all seven `systems::vats::scan_tests::*` with "attempted acquisition of `ActorVitals` while holding `GlobalTransform` … `ActorVitals` -> `RapierHandles` -> `GlobalTransform` -> `ActorVitals`". `BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux --bin byroredux`: 7 of the 11 red tests are this cycle. The same failure is in CI from `c7463c04d` to `fa6be4a6b`.

## Impact
As in ECS-2026-10-27-01, the callers are serial today, so the risk is latent. The lane is red, though, and the function is the template for the Phase-4 VATS work.

## Related
#6429 (the same function's understated exclusive declaration; a fix here should declare the reads it keeps), #6422, #6350, #5954 (gather-then-filter pattern).

## Suggested Fix
Gather `(entity, translation)` from `GlobalTransform` into a `Vec` and drop the guard. Then filter and price each actor with nothing held (the #5954 pattern). `visible_fraction`'s `target_bodies` set can likewise be built once per scan, before the loop.

## Completeness Checks
- [ ] **SIBLING**: Other VATS paths (`vats_confirm`, `vats.queue`) and any multi-storage loop added with #6350 checked for the same held-guard shape
- [ ] **LOCK_ORDER**: TypeId-sorted acquisition preserved; no resource/storage read while the gather guard is alive
- [ ] **TESTS**: All seven `systems::vats::scan_tests::*` pass under `BYRO_LOCK_ORDER_CHECK=1`

