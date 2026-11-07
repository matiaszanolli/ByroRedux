# CHAR-2026-11-07-L-08: The player's CharacterLevel and Background are withheld whenever the derivation yields nothing (Oblivion, also FO76/Starfield)

**Issue**: #6767
**Labels**: bug, low, game:oblivion, character
**Source audit**: `docs/audits/AUDIT_CHARACTER_2026-11-07.md` (finding L-08, Dim 4). Game: Oblivion (FO76 and Starfield profiles also carry NpcStatModel::None).

## Description
`build_player_character_template` returns early with only `factions`, `spells`, `racial_spells`, `perks` when the derivation yields no pairs. `Background { race_form_id, class_form_id }` and the level are pure record provenance and do not depend on the derivation, yet they are computed after the early return. Oblivion has `NpcStatModel::None` and no AVIF, so its Player `NPC_` always takes this branch; the player body root skips the identity stamps, so nothing else stamps them. Oblivion NPCs do carry both components (`stamp_character_components`, unconditional). Residue of #4678.

## Location
`byroredux/src/inventory/catalog.rs:345-354` (the `pairs.is_empty()` early return) against `:362-367`; NPC counterpart `byroredux/src/npc_spawn/stamps.rs:198-228`.

## Evidence
`PlayerCharacterTemplate::level` and `background` are `None` on the early-return path, so `attach_to_player` skips `CharacterLevel` and `Background`. `condition/catalog.rs:999` (`GetIsRace`) and `eval/actor.rs:31` read `Background` and treat absence as false/0; `GetLevel` falls back to 0. Not a numeric finding; the `PlayerCharacterTemplate::background` doc says the value is "exactly what `derive_npc_actor_values` already consumed".

## Impact
On the Oblivion player `GetIsRace`, `GetIsPlayableRace` and `GetLevel` are silently false/0. Loot bootstrap is unaffected (`attach.rs:251` uses `map_or(1, ..)`). Escalate to MEDIUM if a census shows player-targeted `GetLevel`/`GetIsRace` CTDAs in Oblivion content (not run).

## Related
#4678, #3158, #5413.

## Suggested Fix
Compute and stamp `level` and `background` before the `pairs.is_empty()` early return (only `values` and `vitals` need the derivation). Pin with a Player record that derives to nothing but carries race and class.

## Completeness Checks
- [ ] **SIBLING**: FO76 and Starfield (also `NpcStatModel::None`) go through the same path
- [ ] **TESTS**: a Player record that derives to no actor values still gets `CharacterLevel` and `Background`
