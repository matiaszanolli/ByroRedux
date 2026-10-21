# #6329: FO3-2026-10-21-01: The LTEX Havok shipped-master census skips Fallout 3

**Labels**: low,bug,esm-plugin,physics,test-gap,game:fo3,legacy-compat

**Source**: docs/audits/AUDIT_FO3_2026-10-21.md · **Severity**: LOW · **Dimension**: 2 (ESM slice) · **Location**: `crates/plugin/tests/parse_real_esm.rs:1314-1349` (`ltex_havok_data_decodes_on_shipped_masters`)

## Status at filing
NEW (test gap). The FO3 skill names this gap ("FO3's 51 LTEX material bytes are unmeasured").

## Description
#6048's real-data census covers Oblivion (218), FNV (89) and Skyrim (68). FO3 decodes through the same pre-Skyrim `HNAM` arm, but no test pins its count or that its material bytes stay inside the FO3 Havok enum.

## Evidence
raw-byte census of `Fallout3.esm` (independent of the Rust parser):
- All 51 LTEX carry a 3-byte `HNAM`.
- Material bytes: 0 Stone ×7, 2 Dirt ×26, 4 Grass ×6, 10 Heavy Stone ×1, 18 Sand ×3, 19 Broken Concrete ×8.
- Friction and restitution are 30/30 on all 51.
- Examples: `LDirtWasteland01` (`0x15457`) = `02 1e 1e`; `LAsphaltWasteland01` = `13 1e 1e`; `GravelRoadWasteland01` = `12 1e 1e`.

## Impact
the FO3 leg of the decode is unguarded, so an FO3-only regression in the shared arm passes CI and the existing census.

## Related
#6048, `c53d87679`.

## Suggested Fix
add `(FO3_ENV, FO3_DEFAULT, "Fallout3.esm", Some(31), 51)` to the census table, and drop the FO3 caveat from the skill.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
