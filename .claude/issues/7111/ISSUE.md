# #7111 — ESM-D6-01: FO76 `PERK` `EPF2` read as an lstring on every entry type (`CURV` FormID on EPFT 1, int on EPFT 5); 333/409 mis-decoded

Labels: low,esm-plugin,bug,game:fo76
Filed from: docs/audits/AUDIT_ESM_2026-11-20.md (finding ESM-D6-01)

Source: `docs/audits/AUDIT_ESM_2026-11-20.md` (`/audit-esm`, HEAD 47ae285c6 / f3713bf5e) · ID **ESM-D6-01**

- **Severity**: LOW (latent: the only consumer reads `button_label` on Add-Activate-Choice entries, which are EPFT 4)
- **Dimension**: 6 (Localized Strings; the inverse hazard, a non-lstring 4-byte field decoded through the lstring helper)
- **Record / Sub-record**: PERK `EPF2` (FO76)
- **Location**: `crates/plugin/src/esm/records/misc/magic.rs:670-678` (`b"EPF2" => ... details.button_label = read_lstring_or_zstring(&sub.data)`)
- **Status**: NEW
- **Description**: xEdit gives FO76 `EPF2` a decider (`wbEPF2Decider`, `wbDefinitionsFO76.pas:2165-2175`, union at :9790-9793): `Button Label` (lstring) only when `EPFT <> 1`, `Curve Table` (FormID) when `EPFT = 1`; the same struct also declares `wbInteger(EPF2, 'Unknown Spell Value', itU32)` (:9829) and the header comment says `5: EPFD=SPEL, EPF2=unknown int`. TES5/FO4 declare `EPF2` as `wbLString` Button Label unconditionally (TES5.pas:5984, FO4.pas:7662), so the parser is correct there and the game gate is missing for FO76. `read_lstring_in` treats any 4-byte payload of a localized plugin as a table id, so a FormID/int becomes `<lstring 0x…>` or, when it happens to equal a table id, another string.
- **Evidence** (Python walk of SeventySix.esm, `epf2.py`/`epf2b.py`): FO76 `PERK` `EPF2`, all 4 bytes: EPFT 1 = 64 entries, 0 table hits, the 46 distinct values are all FormIDs of `CURV` records (e.g. `0x008F5882`); EPFT 5 = 269 entries (261 miss, 8 hit `.STRINGS` id 4 by coincidence), distinct values are 4 small ints; EPFT 4 = 76 entries, all real `.STRINGS` ids. FO4 (all 21 `EPF2` are EPFT 4) and Skyrim (8 of 8 resolve in `.STRINGS`) are clean. So 333 of 409 FO76 `EPF2` payloads are mis-decoded, e.g. `button_label == "<lstring 0x008F5882>"`.
- **Impact**: wrong/placeholder `PerkEntryDetails::button_label` on 333 FO76 perk entries. `button_label` is read only by `crates/scripting/src/perk_entry_points/activate.rs:57` (function 9, EPFT 4) and the census example, so nothing wrong is visible today; the CURV reference on EPFT 1 is dropped from the decode (only the raw bytes survive in `formatter`).
- **Suggested fix**: gate the lstring read on `EPFT == 4` (the only value where Skyrim/FO4/FO76 all author a label; FO3/FNV keep the zstring) and keep the raw bytes in `formatter`; for FO76 EPFT 1 decode the `CURV` FormID (remapped) if the curve table is ever consumed. Add a localized-plugin test with a FO76 EPFT 1 `EPF2` FormID.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related decoders / other games' arms (per-game layout splits, other callers of the same helper)
- [ ] **TESTS**: A regression test pins this specific fix (synthetic fixture in the default lane; real-data pins stay in the ignored lane)
