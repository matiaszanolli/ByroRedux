# #4814 — GAME-D4-2026-09-24-01: The ACHR "Starts Dead" header flag (0x200) is never decoded — authored corpses spawn alive, cannot be looted, and hostile ones attack

**Labels**: high,gameplay,combat,esm-plugin,bug
**Filed from**: docs/audits/AUDIT_GAMEPLAY_2026-09-24.md

From `docs/audits/AUDIT_GAMEPLAY_2026-09-24.md` (HEAD `aabd99a05`).

- **Severity**: HIGH
- **Dimension**: 4 — Combat & Death
- **Location**:
  - The same parser gap: `walkers.rs:866` and `PlacedRef`.
  - No `Dead` producer for it: there is no `starts_dead` consumer anywhere.
  - `byroredux/src/inventory.rs:687-700`: `is_loot_source` requires `Dead` on actors.
- **Status**: NEW
- **Description**:
  - xEdit's ACHR flag list gives `9, 'Starts Dead'` (`wbDefinitionsTES5.pas:3110`). Skyrim and FO4 use it for every placed corpse (`TreasCorpse*`, `DN027_TheodoreCroupCorpse`).
  - Because nothing reads the flag, the actor spawns as a live NPC with `CombatDisposition`. It stands in an idle pose and is not a loot source.
  - Since #4414, it starts combat if hostile.
- **Evidence**:
  - Skyrim: 1,123 refs with 0x200 (base AIDT: 681 Very Aggressive, 128 Aggressive).
  - FO4: 1,178 refs (428 Very Aggressive, 98 Aggressive).
  - Skyrim `BleakFallsBarrow01` alone has 19: `dunBleakFallsCorpseBretonThomas`, two `TreasCorpseBanditNordMale`, `TreasCorpseSkeleton(Rigid)` and 12 `EncSkeever`.
  - FO3/FNV have no 0x200 actor refs. Their corpse encoding (bases named `DEAD*`/`Loot1*`) was not established, so they are out of scope here.
- **Impact**:
  - Dungeon corpses stand up, and hostile ones attack the player.
  - Treasure corpses can't be looted, so their authored loot is unreachable.
  - This sits on the Skyrim MQ102 critical path.
- **Trigger**: Skyrim `BleakFallsBarrow01`; FO4 `SentinelSite01` or `SuperDuperMart01`.
- **Related**: GAME-D5-2026-09-24-01; #4414.
- **Suggested Fix**: expose the flag. At actor-job completion, insert `Dead` and run `reconcile_dead_actor` (the path `reference_state::restore` already uses), so the ragdoll settles and the corpse becomes a loot source. Pin it with a Starts Dead spawn test.

### MEDIUM

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (both NPC spawn paths, sibling interaction kinds, other parked `ReferenceState` facts)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved (no guard held across `world.get`/physics queries)
- [ ] **SAVE**: If a saved shape changes, `FORMAT_MAJOR` is bumped and no `serde(default)` is added (#4465)
- [ ] **TESTS**: A regression test pins this specific fix
