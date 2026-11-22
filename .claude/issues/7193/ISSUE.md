# #7193 CHAR-2026-11-21-M-01: Oblivion PC-level-offset actors expand leveled gear and spells at the FO3/FNV `calc_min` stand-in, not at the actor's own (player-relative) level

**Labels**: medium,character,bug,game:oblivion

From `docs/audits/AUDIT_CHARACTER_2026-11-21.md` (`/audit-character`, HEAD `a06d5f9db`).

- **Reported as**: CHAR-D4-01
- **Severity**: MEDIUM
- **Dimension**: Population Boundary
- **Game**: Oblivion
- **Location**: `crates/plugin/src/esm/records/actor/mod.rs:216-222` (`effective_actor_level`), `crates/plugin/src/equip.rs:857-863` (`ResolvedNpc::level`), `crates/plugin/src/equip.rs:1008-1030` (`resolve_actor_spells`, `resolve_racial_spells`), `byroredux/src/npc_spawn/equip.rs:234` (`build_npc_equip_state`: `let actor_level = resolved.level();`); the correct level already exists at `crates/plugin/src/esm/records/oblivion_population.rs:40-46` (`actor_level_at_spawn`)
- **Status**: NEW (adjacent: #7022 observes flag timing only; #2955/#4812/#4232 closed, FO3/FNV-only semantics)
- **Source**: `/mnt/data/src/reference/uesp-wiki/Oblivion/Leveled Lists.wiki:109-110` ("The equipment carried by NPCs is determined by the lowest level between the NPC in question and the player") and `/mnt/data/src/reference/cs-uesp-wiki/(main)/How Oblivion engine deals with NPC leveled equipment.wiki` (the experiment: player 31, offset -21 -> level 10 gear; player 10, offset +20, min 30 -> still player-level gear). Level semantics of ACBS bit 7 on Oblivion: `charal-oblivion-ruleset.md` "NPC auto-calc stats" (player level + offset, clamped, floor 1) and `Oblivion.esm` ACBS read as `level: i16` offset.
- **Description**: `effective_actor_level` encodes the FO3/FNV meaning of ACBS bit 7 ("PC Level **Mult**": `level` is a x1000 multiplier, confirmed on FalloutNV.esm in its docstring) and answers `calc_min.max(1)` whenever the bit is set. On Oblivion the same bit is "PC Level **Offset**" and `level` is a small signed offset. The Oblivion population path was given the right level (`actor_level_at_spawn`: player level + offset, min/max clamps, floor 1) for stats and `CharacterLevel`, but every leveled-list expansion (inventory/outfit LVLI, LVSP spells, racial spells) still reads `ResolvedNpc::level()` -> `effective_actor_level`. So one Oblivion actor carries two levels: stats at `player + offset`, gear and spells at `calc_min` (or 1).
- **Evidence**: `Oblivion.esm` census (read-only byte scan of ACBS): 1,568 of 2,482 `NPC_` (63%) and 284 of 914 `CREA` set bit 7; of the 1,568 NPCs, 496 author `calc_min == 0` (gear level 1 at any player level) and 1,072 author a non-zero `calc_min` that then fixes their gear tier for the whole game. `ResolvedNpc::level()` has no game or stat-model arm; `build_npc_equip_state` passes only `resolved` and `index`.
- **Impact**: generic bandits, guards and most creatures with leveled inventories draw the lowest eligible tier regardless of the player's level (expected: `min(actor level, player level)`), so a level-20 player meets level-1 equipment on the majority of NPCs; spells from LVSP likewise.
- **Related**: #7022, #7040 (population work), CHAR-D4-04; this is not the #6768 split (that one is level-0 LVLN picks on FO3/FNV).
- **Suggested Fix**: make the leveled-list level a per-stat-model answer (the `NpcStatModel` already selects `OblivionNpc`/`OblivionCreature`), computed as `min(actor_level_at_spawn(npc, player_level), player_level)` per the two sources above; keep `effective_actor_level` for the Fallout multiplier semantics and say in its docstring that bit 7 is an offset on Oblivion. Add a census test over the vanilla offset actors (player level 1 and 30) beside `oblivion_population_census_stamps_every_standard_value`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other CHARAL rule rows / overlays / profiles)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
