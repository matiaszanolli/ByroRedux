# #4325 SCR-D6-2026-09-14-02: `npc_combat_ai_system` holds `PhysicsWorld` across every storage it acquires, reversing `ragdoll_writeback_system`'s `Transform → PhysicsWorld` edge

**Labels**: medium,concurrency,combat,bug
**Source**: `docs/audits/AUDIT_SCRIPTING_2026-09-14.md`

- **Severity**: MEDIUM (not a live deadlock: both systems are exclusive and run in different stages; trips the lock-order checker and breaks the rule #2134 / #3262 / #3655 enforced)
- **Dimension**: Scripting Runtime Systems
- **Untrusted-Input**: No
- **Location**: `byroredux/src/systems/combat_ai.rs` `npc_combat_ai_system` (`world.try_resource::<PhysicsWorld>()` at the top, alive through `query::<AiCombatState>`, `query::<Transform>`, `attack_damage`'s storages, and phase 2's `query_mut::<Transform/AiCombatState/HitEvent>`); `byroredux/src/ragdoll.rs` `ragdoll_writeback_system` (Transform/Parent/Children/GlobalTransform/LocalBound/WorldBound held, `PhysicsWorld` taken last)
- **Status**: NEW (#3580 was a different system's `PhysicsWorld` hold)
- **Description**: The crate-wide rule, per the #3655 comment: no storage is acquired under a `PhysicsWorld` guard. The combat AI's module doc claims to mirror `wander_system_inner`, but wander takes `PhysicsWorld` only after its storage guards drop. Every frame (the `AiCombatState` storage is always registered) the combat AI records `PhysicsWorld → Transform`, while `ragdoll_writeback_system` records `Transform → PhysicsWorld`.
- **Evidence**: The orchestrator confirmed the acquisition order: `combat_ai.rs:50` `PhysicsWorld`, then `:56` `Transform`, versus `ragdoll.rs:507` `Transform`, then `:529` `PhysicsWorld`. The lock tracker records edges on both read and write. The resulting panic under `BYRO_LOCK_ORDER_CHECK=1` is derived from the code, not observed (no engine launch).
- **Impact**: A debug run with the lock-order checker enabled panics at boot, which blinds the tool that gates promoting a system to a parallel lane. Moving either system to a parallel lane would turn this into a real ABBA risk.
- **Related**: #2134, #3262, #3655, #2404
- **Suggested Fix**: Restructure like #2134 / #3262. Phase 1a snapshots actor and target transforms, reach, damage and cooldown with no `PhysicsWorld` held. Phase 1b takes `PhysicsWorld` alone for `step_toward`. Drop it before the phase-2 `query_mut`s.

_Source: `docs/audits/AUDIT_SCRIPTING_2026-09-14.md`_

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other primitives / spawn paths / walkers)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved and no storage is acquired under a `PhysicsWorld` guard
- [ ] **TESTS**: A regression test pins this specific fix
