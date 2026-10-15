# #5860: GAME-D4-2026-10-15-02: Projectile and `ProcessTrapHit` producers overwrite a same-frame `HitEvent` (regression of #4324)

- **Labels**: medium,bug,gameplay,combat
- **Filed from**: `docs/audits/AUDIT_GAMEPLAY_2026-10-15.md`

- **Severity**: MEDIUM
- **Dimension**: Combat & Death (Dim 4)
- **Location**: `byroredux/src/systems/projectiles.rs:284-299`; `crates/scripting/src/fragment/effects/deferred.rs:654-673` (`flush_trap_hits`)
- **Status**: Regression of #4324 (the single-slot `HitEvent` overwrite, fixed for `npc_combat_ai_system` at `combat_ai.rs:262-280`)
- **Trigger**: any game. A dart-trap or `Weapon.Fire` hit lands on an actor in the same frame as a player or NPC melee hit, or two trap hits land together.
- **Description**: `HitEvent` is one SparseSet slot per target. #4324 made the NPC producer hold its strike when `events.get(target).is_some()`. Both new producers, registered after `combat_input_system` and `npc_combat_ai_system`, call `events.insert(target, …)` unconditionally, replacing the earlier hit. `hit_event_consumers_run_after_every_producer` pins only the order, not the overwrite.
- **Evidence**: `projectiles.rs:287-298`: `for (target, shooter, damage) in hits { … events.insert(target, HitEvent { … }) }`, with no existing-row check.
- **Impact**: Melee damage is silently lost when a projectile or trap hit lands the same frame, and several trap hits collapse into one.
- **Suggested Fix**: Hold or merge as `combat_ai.rs` does, or make `HitEvent` a per-target queue. Add a test with a melee hit and a projectile hit landing on one target in one frame, asserting both damages apply.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the sibling producers/paths named in the finding
- [ ] **LOCK_ORDER**: If a RwLock/query scope changes, TypeId-sorted acquisition and the system's declared `Access` row are preserved
- [ ] **TESTS**: A regression test pins this specific fix
