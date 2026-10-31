# GAME-D7-2026-10-31-01: The `Access` rows of `interaction_system`, `container_loot_system` and `combat_damage_system` omit what M43 made them acquire

Labels: low,gameplay,ecs,bug

Filed from `docs/audits/AUDIT_GAMEPLAY_2026-10-31.md` (finding `GAME-D7-2026-10-31-01`).

- **Severity**: LOW
- **Dimension**: Stage Order & Declared Access (Dim 7)
- **Location**: `byroredux/src/boot/schedule/update/interaction_and_crime.rs:75-117` and `:140-175`; `byroredux/src/boot/schedule/update/player_animation.rs:21-84`
- **Status**: NEW (M43 commits 851636a13, 2c55546f4, ccccbaf03, 9e17ebce1, 31aca901d, 9c6c37cd0)
- **Description**: The three exclusives keep a declared `Access` row that the comments call the report the analyzer uses. `interaction_system` now also reads `CraftingBook`, `LockpickItem`, `TerminalBases`, `SmTree` and `ObScriptLedger` (the jail bed candidates), writes `CraftingRequest`, and writes `StoryEvent` and `PendingScriptStoryEvents` through `unlock_with_carried_key` (`LOCK`/`HACK`). `container_loot_system` writes `StoryEvent` (`AIPL`, `loot.rs:270,534`). `combat_damage_system` reads `ArmorBook`, `ItemInstancePool`, `ActorValueNames` (`armor::mitigate`) and writes `StoryEvent` (`ASSU`; `KILL` since #5434). A grep of `boot/` finds none of those types in those three rows. Exclusives are not paired by the analyzer, and `assert_declares_everything_it_acquires` covers only the parallel systems and four named exclusives, so nothing fails.
- **Impact**: The `sys.accesses` report under-describes the busiest gameplay systems, which is the exact complaint #3473 fixed for the first P2 set. No behavioural effect today.
- **Suggested Fix**: Add the missing types. Add `interaction_system`, `container_loot_system` and `combat_damage_system` to a declares-everything test next to `npc_combat_ai_system_declares_everything_it_acquires`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other games / siblings)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
