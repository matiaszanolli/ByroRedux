# #7319 ECS-D5-02: New declared exclusive rows under-declare, and no guard covers them

Labels: low,ecs,concurrency,bug

From `docs/audits/AUDIT_ECS_2026-11-22.md` (HEAD e2a4c54cc; re-verified at f5329b2f5).

- **Severity**: LOW
- **Location**: `update/interaction_and_crime/detection.rs` (`detection_tick_system`), `early.rs` (`sneak_stance_system`), `update/combat/combat_input.rs` (`player_cast_system`, `combat_input_system`).
- **Status**: NEW
- **Description**: the acquisition guard covers only 8 named exclusive bodies. The rows below are missing types their bodies take:
  - `detection_tick_system`: the `StealthPoints` write and the `ActorValueNames` read.
  - `sneak_stance_system`: the `StanceLocomotion`, `WalkAnimation` and `AnimationPlayer` writes.
  - `player_cast_system`: the `ActiveCamera`, `Transform` / `GlobalTransform`, `PhysicsWorld` and `RapierHandles` reads, and the `ActiveProjectiles` write. Its sibling `player_bow_system` declares these.
  - `combat_input_system`: the `PlayerActionNoise`, `ActiveProjectiles` and `Inventory` writes, and the `StealthRules`, `InventoryCatalog`, `Sneaking`, `Awareness`, `AiCombatState` and `ActorValueNames` reads.
- **Suggested Fix**: drive the exclusive guard from every `add_exclusive_with_access` row, not a named list, and then fill in the missing types.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other multi-storage gathers / other rows / other producers)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, the process-wide order (and TypeId-sorted pair acquisition) is preserved; the `BYRO_LOCK_ORDER_CHECK=1` lanes (core, scripting, bin) stay green
- [ ] **TESTS**: A regression test pins this specific fix (for a lock-order fix: a fixture that holds every storage involved, so the detector lane can see the edge)
