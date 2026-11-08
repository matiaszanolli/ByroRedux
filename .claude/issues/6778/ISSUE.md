# ESM-D7-01: Starfield ARMO/ARMA carry `BO64` (u64), which no decoder reads; `equip.rs` documents Starfield as "BOD2 u32 ... unsourced"

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/6778
Labels: medium, esm-plugin, bug, game:starfield
Source: docs/audits/AUDIT_ESM_2026-11-08.md

_Filed from `docs/audits/AUDIT_ESM_2026-11-08.md` (audit HEAD 3500d2737)._

**Related**: Closed #4074 only marked the Starfield/FO76 `main_body_bit` arms provisional.

- **Severity**: MEDIUM (a whole game's armour biped masks decode to 0; caveat: I did not run a Starfield NPC dress-up end to end)
- **Dimension**: 7 (equip biped-slot constants) / 2 (sub-record decode)
- **Record/Sub-record**: ARMO, ARMA `BO64` (Starfield)
- **Location**: `crates/plugin/src/esm/records/items.rs:676-690` (ARMO reads only `BMDT`/`BOD2`/`BODT`), `crates/plugin/src/esm/records/misc/equipment.rs:100-112` (ARMA, same); `crates/plugin/src/equip.rs:20-36` (table column "Starfield (BOD2 u32)"), `:38-71` (provisional note), `:101-120` (`main_body_bit`)
- **Status**: NEW (closed #4074 only marked the arm provisional)
- **Description**: xEdit's Starfield definition has no `BOD2` on ARMO/ARMA. Both records use `wbBO64` (`wbDefinitionsSF1.pas:9631` ARMA, `:9724` ARMO), a single `itU64` `First Person Flags`. `wbBipedObjectFlags` (`:2474`) is a different bit table from FO4's: 0 Hide Hair, 1 Morph Hair, 2 Hide Head, **3 BODY**, 4 Hat, 5 Backpack, 6 Gloves, 7..15 "Vis Only" slots, 61..63 unused/FaceLights. `grep -rn BO64 crates/plugin/src` returns nothing, so `ItemKind::Armor.biped_flags` and `ArmaRecord.biped_flags` stay 0 for every Starfield armour.
- **Evidence**: `dump_record_subs Starfield.esm ARMO Spacesuit_Constellation` (Starfield.esm, this HEAD): every record has `BO64 len=8` and no `BOD2`. Read as `u64::from_le_bytes`, the values land on xEdit's flag table (`wbDefinitionsSF1.pas:2474-2539`): `Spacesuit_Constellation_01` (001E2B18) = `00 00 00 00 08 00 00 00` = `0x0000_0008_0000_0000` = bit 35 **"SS BODY"**; `Spacesuit_Constellation_Helmet_01` (001E2B17) = bit 36 "SS Helmet"; `Spacesuit_Constellation_Backpack_01` (001E2B19) = bit 37 "SS Backpack". The low-dword "non-suit" set (3 BODY, 4 Hat, 5 Backpack) is a separate namespace from the "SS" (spacesuit) set in the high dword.
- **Impact**: `ItemKind::Armor.biped_flags` / `ArmaRecord.biped_flags` are 0 for every Starfield armour, so `npc_spawn/equip.rs:396` `equipment_slots.equip(biped_flags, ..)` occupies no slot and `main_body_covered` (`npc_spawn/equip.rs:177` via `main_body_bit`) is always false. Worse, the "Some(3) is right by coincidence" reading is wrong: the vanilla spacesuit body claims bit **35**, which a `u8` position `0..32` / u32 mask cannot express, so even after `BO64` is read, `main_body_bit(Starfield) == Some(3)` would miss every spacesuit body and only match non-suit clothing.
- **Suggested fix**: (1) read `BO64` (cast to the u64, keep 64 bits or fold to the u32 the consumers use) in ARMO and ARMA when `game == Starfield`; (2) widen the Starfield biped mask to u64 and make `main_body_bit` answer both bit 3 (BODY) and bit 35 (SS BODY), cited to `wbDefinitionsSF1.pas:2474`; drop Starfield from the PROVISIONAL set; (3) fix the module table header. The FO76 arm can be cited now too: `/mnt/data/tmp/esmaudit/FO76.pas:7214-7226` (`wbBipedObjectFlags`, bit 3 = `33 - BODY`) and `:7252` (`BOD2` = one `U32`) match FO4. Retire `fo76_and_starfield_arms_are_marked_provisional` with it (its own doc says to).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related decoders / other games' arms
- [ ] **LOCK_ORDER**: n/a unless a RwLock scope changes
- [ ] **TESTS**: A regression test pins this specific fix (real-data pin where an install exists)
