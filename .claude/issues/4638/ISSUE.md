# ESM-2026-09-21-D2-01: Oblivion's legacy leveled-list encodings are not normalized — 8-byte LVLO rows dropped, LVLD 0x80 flag read as chance-none 128 (18 LVLIs expand to nothing)

**Issue**: #4638
**Filed**: 2026-09-22 (audit-publish, AUDIT_ESM_2026-09-21.md)

**Severity**: MEDIUM
**Dimension**: Sub-Record Byte Accounting
**Game Affected**: Oblivion
**Location**: `crates/plugin/src/esm/records/container.rs:203` (`LVLD` → `chance_none = sub.data[0]`), `crates/plugin/src/esm/records/container.rs:235-246` (`b"LVLO" if sub.data.len() >= 12`); consumer `crates/plugin/src/equip.rs:788`

## Description
`parse_leveled_list_for_game` has two problems with legacy TES4 encodings. (a) xEdit's shared `wbLeveledListEntry` makes `Count` optional — a TES4 `LVLO` can be 8 bytes (Level, pad, Reference), and the decoder's `>= 12` guard drops such rows entirely. (b) xEdit's TES4 `wbLVLAfterLoad` treats the high bit of `LVLD` as the legacy "Calculate from all levels" flag: mask it off chance-none and OR `0x01` into `LVLF`. Redux stores the raw byte instead.

## Evidence
- xEdit: `wbDefinitionsCommon.pas:9062-9076`, `wbDefinitionsTES4.pas:755-775`.
- Real data, `Oblivion.esm`: 94 LVLO rows are 8 bytes long across 14 lists (all decode to `entries = []`): TesKvatchCreature, OblivionContainerArmor01/02, OblivionContainerWeapon01, FGC03ThiefWeapons/Armor, Dark02/03/05/06RewardX, ArenaYellowArmor, ArenaBlueArmor, ArenaRandomShields/Helmets. 11 lists have `LVLD = 128`: 6 LVLC + 5 LVLI (ArenaLeveledGold*, Dark07RewardFinery).

## Impact
NPC inventory/outfits (`expand_leveled_form_id`) and container loot (`expand_leveled_loot`) resolve the 14 affected lists to nothing; `equip.rs:788`'s `chance_none >= 100` check empties the 5 LVLD=128 LVLIs too. Players see Arena combatants and Fighters Guild thieves without gear, empty gate loot, empty Dark Brotherhood rewards.

## Suggested Fix
Accept 8+ byte `LVLO` rows (Count only when ≥10 bytes, default 1). Mask `LVLD & 0x7F`, OR `0x01` into flags when bit 7 set. Pin both with fixtures.

## Related
#4248 (open, different defect — FO4 container LVLI expansion). Cited as background by `AUDIT_GAMEPLAY_2026-09-21.md`'s corpse-loot finding; no gameplay-track issue existed at publish time to cross-link.

## Source
docs/audits/AUDIT_ESM_2026-09-21.md (ESM-2026-09-21-D2-01)
