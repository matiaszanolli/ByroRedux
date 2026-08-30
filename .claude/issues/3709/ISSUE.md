# #3709 — ECS-2026-08-30-P2-06: per-actor melee state (cooldown_remaining, blocking) lives in the global CombatState Resource

*Filed 2026-08-30 from `docs/audits/`. Immutable snapshot of the issue as filed (TD10-001 / #1156); GitHub is authoritative for current state.*

**Severity**: LOW · **Dimension**: P2 Gameplay Slice / Resource shape
**Location**: `byroredux/src/combat.rs` (`CombatState`, ~:45-69)
**Source**: `docs/audits/AUDIT_ECS_2026-08-30.md` (ECS-P2-06, `[P2-gameplay]`)

> **Coverage note**: this file has no owner audit skill. The finding comes from the `/audit-ecs` run's explicit P2-gameplay slice sweep and is the only audit coverage it received.

## Description

`CombatState` mixes genuinely global telemetry (`attacks_started` / `hits_landed` / `kills` / `last`) with two per-combatant facts (`cooldown_remaining`, `blocking`), both written from the player's `ActionState`.

## Impact

**No present-day failure** — there is exactly one combatant (`combat_input_system` resolves its aggressor from `PlayerEntity` and gates on `PlayerMode::Character`), and the code already flags that `HitEvent::blocked` has no NPC-side producer. LOW because it is a shape issue: the first NPC attacker makes a single global cooldown/block flag incorrect.

## Suggested Fix

Split the two per-actor fields into a `MeleeState` component (`SparseSetStorage`) on the combatant, leaving `CombatState` as pure session telemetry.

## Completeness Checks
- [ ] **SIBLING**: Other gameplay `Resource`s checked for per-actor fields (e.g. `InteractionTrace`)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test drives two simultaneous attackers and asserts independent cooldowns
