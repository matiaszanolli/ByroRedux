# #4324 SCR-D6-2026-09-14-01: `HitEvent` now has two producers, both `insert`ing into the target's single SparseSet slot, so same-frame hits overwrite each other

**Labels**: medium,scripting,combat,gameplay,bug
**Source**: `docs/audits/AUDIT_SCRIPTING_2026-09-14.md`

- **Severity**: MEDIUM
- **Dimension**: Scripting Runtime Systems
- **Untrusted-Input**: No
- **Location**: `byroredux/src/systems/combat_ai.rs` `npc_combat_ai_system` phase 2 (`events.insert(target, HitEvent{..})`); `byroredux/src/combat.rs` `combat_input_system` (`events.insert(target, ..)`); stale "exactly one HitEvent producer" comments in `byroredux/src/combat.rs` and `byroredux/src/npc_spawn.rs`
- **Status**: NEW. This is the same defect class as #1864 / #3277 on `QuestStageAdvancedBatch`, fixed there with a merging helper.
- **Description**: `HitEvent` is `SparseSetStorage` with one row per target, and `insert` replaces the row. Schedule is `combat_input_system` (`update.rs` ~127) → `npc_combat_ai_system` (~157) → `combat_damage_system` (~175). If the player and an AI strike one target in the same frame, the AI's insert replaces the player's hit. If several NPCs strike one target in the same frame, all but the last are dropped. NPCs armed by one fragment start with identical cooldowns and stay synchronized, so the loss persists for the whole fight.
- **Evidence**: The orchestrator confirmed both producers use `events.insert(target, byroredux_scripting::HitEvent{..})` with no existing-row check, that `HitEvent` is `SparseSetStorage`, and the schedule order. All combat_ai tests use a single attacker.
- **Impact**: Damage and on-hit script events (including the extension OnHit path) are silently lost in MQ101's Helgen combat gate (stages 270/272/365).
- **Related**: #3277, #1864, #4105
- **Suggested Fix**: Make the per-target hit list-valued (a `HitEventBatch`, or accumulate into an existing row) behind one push helper both producers call, and fix the two comments.

_Source: `docs/audits/AUDIT_SCRIPTING_2026-09-14.md`_

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other primitives / spawn paths / walkers)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved and no storage is acquired under a `PhysicsWorld` guard
- [ ] **TESTS**: A regression test pins this specific fix
