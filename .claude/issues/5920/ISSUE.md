# CHAR-2026-10-16-M-07: The player never receives a `Perks` component from the Player `NPC_`; Skyrim's Player record authors 12 perks

Labels: medium,character,bug,game:skyrim
Filed from: docs/audits/AUDIT_CHARACTER_2026-10-16.md

- **Severity**: MEDIUM
- **Dimension**: Population Boundary (writers vs stamps)
- **Game**: skyrim (FO4 by lineage; FO3/FNV/Oblivion author none)
- **Location**: `byroredux/src/inventory.rs` `PlayerCharacterTemplate` / `build_player_character_template` / `attach_to_player` (no `perks` field, no insert); NPC counterpart `byroredux/src/npc_spawn.rs:317` `stamp_character_components`
- **Status**: NEW (Related: #3158, closed — fixed the NPC half; `crates/plugin/src/esm/reader.rs` `uses_npc_perk_entries` still says "the remaining gap there is the player")
- **Source**: not numeric. `Skyrim.esm` Player `NPC_` 0x7 carries `PRKZ=12` and 12 `PRKR` entries (dumped): `PerkSkillBoosts` 0xCF788, `AlchemySkillBoosts` 0xA725C, `AllowShoutingPerk` 0xF11A9, `TorchBashPerk` 0xFE304, `PlayerUnderforgeAccessPerk` 0xAEC05, `VampireFeed` 0xCF02C, `PlayerWerewolfFeed` 0x2BA1D, `DBWellFitted`, `TGWellFitted` and others, all rank 1.
- **Description**: `PlayerCharacterTemplate` carries values, vitals, level, background, factions, spells and racial spells but no perks. Every production writer of the player's `Perks` is on demand (`levelup perk`, script `AddPerk`/`ChangePerk`). Readers (`HasPerk` CTDA, `perk_entry_points::compose_entry_point` — strike damage, Max Carry Weight, XP, DT, skill points — and consumable conditions) see an absent component. All unit tests hand-insert `Perks` on the player (`inventory.rs` tests ~2057, 2197, 2461, 2605), which hides the gap (#4458/#5413 class).
- **Evidence**: `grep '\.perks\b'` in `byroredux/src` finds only `npc_spawn.rs:317`; `attach_to_player` stamps no perks (re-checked: the only `perks` hits in `inventory.rs` are test inserts).
- **Impact**: The Skyrim player's innate perks do nothing: `HasPerk AllowShoutingPerk`-style CTDAs read false and the 0ea4a87d4 entry-point composition cannot see them.
- **Related**: M-03, L-03.
- **Suggested Fix**: Add `perks: Perks` to `PlayerCharacterTemplate` from `resolved.shell.perks` (same source and "ride no template flag" rule as `stamp_character_components`), insert in `attach_to_player`, and add an attach test that does not hand-insert `Perks`.
- **Note**: Follow-up to closed #3158 (fixed the NPC half only).

---
Filed from `docs/audits/AUDIT_CHARACTER_2026-10-16.md` (HEAD fa7591ca2).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other games' rulesets, sibling writers/consumers)
- [ ] **TESTS**: A regression test pins this specific fix
