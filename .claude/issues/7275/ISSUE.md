# #7275 ESM fold: partial forms (record flag 0x4000) replace the record they override whole

State: OPEN
Labels: bug, high, game:fo4, game:starfield, esm-plugin

- **Severity**: HIGH
- **Dimension**: audit-esm load-order fold / LMC-REC-002, LMC-REC-003
- **Location**: `crates/plugin/src/esm/records/index.rs` (`EsmIndex::merge_from`), `crates/plugin/src/esm/reader.rs` (record header flags)
- **Status**: NEW
- **Description**: Fallout 4 and Starfield write some overrides as **partial forms**: record flag `0x4000`, "Partial Form" in xEdit's `wbDefinitionsFO4.pas` and `wbDefinitionsSF1.pas` (also defined in `wbDefinitionsTES5.pas`). A partial form carries only the sub-records it changes. Nothing in the parser or the fold reads the flag. A partial override is decoded like any record and replaces the previous record whole, so every sub-record it leaves out reads as cleared.
- **Evidence** (installed data, 2026-11-22):

  | Data | `LCTN` | `CELL` | `WRLD` | `QUST` |
  |---|---|---|---|---|
  | Starfield DLC + Creations | 49 | 40 | 15 | 7 |
  | Fallout 4 DLC + Creation Club | 19 | 0 | 0 | 7 |
  | Sim Settlements 2 + expansions | 67 | 235 | 3 | 0 |
  | Skyrim SE + DLC | 0 | 0 | 0 | 0 |

  - `SFBGS003.esm` `LCTN 00010DFB` carries only `EDID` and the `ACPR`/`ACUN`/`ACSR`/`ACID`/`ACEP` tables. After the fold, the location has no name, keywords or parent; the master has `FULL`, `KWDA`, `PNAM` and more.
  - `SFBGS00D.esm` partial `CELL 000F4ED4` carries only `EDID`.
  - `DLCNukaWorld.esm` partial `QUST 0005DEE4` carries `EDID` and `VMAD`.
- **Impact**:
  - Measured: locations lose name, keywords and parent. That breaks `LocationHasKeyword` and the location hierarchy that `GetInCurrentLoc` reads.
  - To check: partial cells and quests are likely emptied the same way (cell data and lighting; quest stages and aliases), depending on how `EsmCellIndex::merge_from` and the quest map fold them.
  - It also blocks LMC-REC-003 for locations and quests. A per-field merge would read every absent sub-record as a removal; measured on SS2, it wipes the keywords, name and parent of the FO4 locations SS2 touches.
- **Related**: #6670 (LMC Phase 1), #7276 (the `LCTN` reference tables)
- **Suggested Fix**:
  1. Find a source for the game's rule first (no guessing). xEdit defines the flag but not the runtime rule; the local CK and wiki dumps don't mention it.
  2. Likely shape: overlay a partial form's sub-records on the previous carrier's raw record before decoding. Each sub-record type present in the partial form replaces that type's group; absent types are inherited.
  3. Keep the flag in the fold's provenance, so the conflict report can say an override is partial.
  4. Then extend LMC-REC-003 to `LCTN` and `QUST`.

## Completeness Checks
- [ ] **SOURCE**: the runtime rule for a partial form is cited
- [ ] **TESTS**: a partial `LCTN` override keeps the master's name, keywords and parent; a partial `CELL` keeps its data
- [ ] **REPORT**: `plugins.conflicts` marks a partial override

## Comments
