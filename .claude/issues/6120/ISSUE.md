# SF-SD-02: Starfield `NPC_` `ACBS` (16 bytes) matches no parse arm; every Starfield NPC has `acbs_flags = 0` and `template_flags = 0`

Labels: high,esm-plugin,character,bug,game:starfield,legacy-compat

Severity HIGH: wrong gender, no template inheritance, no essential / unique flags on 100% of Starfield NPCs; the data is there and is dropped.

## Description
Starfield's `NPC_.ACBS` is the FO4 layout minus the trailing `bleedout_override` and `unknown` words: `flags(u32), xp_offset(i16), level(i16), calc_min(u16), calc_max(u16), disposition(i16), template_flags(u16)`, 16 bytes. The NPC parser (`crates/plugin/src/esm/records/actor/npc.rs`) has arms for Oblivion (`GameKind::Oblivion`, `>= 16`), FO4 (`GameKind::Fallout4`, `>= 20`), Skyrim (`>= 24`) and FNV / FO3 (`>= 24`); no arm matches Starfield, so `acbs_flags`, `level`, `calc_min`, `disposition_base` and `template_flags` keep their defaults (re-validated against current code). The unit tests (`actor/tests.rs`) have no Starfield `ACBS` case.

Area consumers affected: `byroredux/src/npc_spawn/resumable/prebaked.rs:102` (`Gender::from_acbs_flags(traits.acbs_flags)`), `byroredux/src/cell_loader/references/mod.rs` (race from `resolve_inherited_traits`, which keys on `template_flags`), `byroredux/src/npc_spawn.rs:517` (`npc_skeleton_path`).

## Evidence (`Starfield.esm`)
- All 7,131 `NPC_` `ACBS` are exactly 16 bytes.
- Template-flag offset: of 45,738 non-zero `TPTA` entries on 5,168 NPCs, the bit `i` of the `u16` at `ACBS+14` is set for every one of them, at no other offset.
- Flag bit 0 is the FO4 female bit: 2,021 of 7,131 `NPC_` records.
- Placed actors (9,530 `ACHR`, all resolve to an `NPC_`): 4,219 female (44%), 6,326 Use Traits, 7,321 Use Stats, 5,516 Use Inventory, 8,105 any template flag. Cydonia (`citycydoniamainlevel`, 75 `ACHR`): 29 female.
- Secondary (unverified, not claimed): Starfield's `TPTA` is 60 bytes (15 template slots; entries 13 and 14 are non-zero on 2,528 and 3,759 NPCs) while the parser stores only the first 13 under the FO4 `wbTemplateFlags` order. Whether the first 13 keep FO4's meaning is not established (no xEdit SF1 definitions locally); do not guess it.

## Impact
Every female Starfield NPC spawns male (skeleton, race default skin, equip gender filter). Templated actors (the majority) do not inherit race / traits, stats, factions, AI or inventory, so the shell record's own, often incomplete, values are used. The essential / unique / protected `acbs_flags` bits are 0, so essential-actor bleedout (M47.2) never sees Starfield's essential NPCs.

## Related
CHAR-SD-01 (#6080: gender read from the shell instead of the Use-Traits terminal; assumes `acbs_flags` is decoded), SF-SD-03 (skeleton), #5498 (TPTA), #4639.

## Suggested Fix
Add Starfield to the FO4 `ACBS` arm with a `>= 16` length gate (template flags at `+14`, level at `+6`). Add a Starfield `ACBS` fixture test (female bit, Use Traits). Settle the 15-slot `TPTA` layout separately.

Source: docs/audits/AUDIT_STARFIELD_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other readers/builders of the same field, other games sharing the code path)
- [ ] **TESTS**: A regression test pins this specific fix
