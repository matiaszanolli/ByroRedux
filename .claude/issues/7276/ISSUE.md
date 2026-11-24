# #7276 LCTN: the Added / Master / Removed reference tables are read as one table, and RCPR as present

State: OPEN
Labels: bug, medium, scripting, game:fo4, game:skyrim, game:starfield, esm-plugin

- **Severity**: MEDIUM
- **Dimension**: audit-esm `LCTN` schema / scripting location runtime
- **Location**: `crates/plugin/src/esm/records/misc/location.rs` (`parse_lctn`), `crates/scripting/src/location.rs` (`LocationTable::from_index`)
- **Status**: NEW
- **Description**: `parse_lctn` treats the `A*` and `L*` reference tables as one table under two spellings. Its module docs say "the names changed with Dawnguard". xEdit names them as distinct deltas:
  - Added (by this plugin): `ACPR`, `ACUN`, `ACSR`, `ACID`, `ACEP`
  - Master (the master's own): `LCPR`, `LCUN`, `LCSR`, `LCID`, `LCEP`
  - Removed: `RCPR`, `RCSR`

  Sources: `wbDefinitionsTES5.pas:6310-6336`, `wbDefinitionsFO4.pas:8045-8154`. `RCPR` (Removed Persist Location References) is read as references whose editor location *is* this location: the opposite of its meaning.
- **Evidence**:
  - `Skyrim.esm` `LCTN 00018A49` has `LCPR`/`LCUN`/`LCSR`; `Dawnguard.esm`'s override has `ACUN`/`ACSR` and no `LCPR`. After the whole-record fold, Whiterun's `editor_location_refs` holds only Dawnguard's added uniques, and the master's persistent references lose their editor location.
  - The same pattern appears in Fallout 4 (`Fallout4.esm` `LCTN 00002CEF` `LCPR`/`LCUN` vs `ss2.esm` `ACPR`/`ACUN`) and in the Starfield DLC.
- **Impact**: `GetInCurrentLoc`, the editor location of a persistent reference, and location-alias fills read an incomplete table whenever a DLC or mod overrides a location. This applies to Skyrim with its DLC too, not only to partial forms.
- **Related**: #6670, #7118 (Starfield `LCTN` stride), #7275 (partial forms)
- **Suggested Fix**:
  1. Decode the three tables apart.
  2. Have the fold combine them as the game does: Master from the defining record, Added and Removed from each override. Which overrides contribute (every carrier, or only the winner) needs a source first.
  3. Until then, at least stop reading `RCPR` entries as present.

## Completeness Checks
- [ ] **SOURCE**: how the game combines Added / Master / Removed across plugins is cited
- [ ] **TESTS**: Whiterun with Dawnguard keeps the master's persistent references' editor location; an `RCPR` entry removes a reference

## Comments
