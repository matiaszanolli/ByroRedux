# ESM-D2-04: Skyrim PERK Quest-entry `DATA` stage is read as a u16 on a length test, but TES5 declares a u8 plus 3 unused bytes

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/6784
Labels: low, esm-plugin, bug, game:skyrim
Source: docs/audits/AUDIT_ESM_2026-11-08.md

_Filed from `docs/audits/AUDIT_ESM_2026-11-08.md` (audit HEAD 3500d2737)._

- **Severity**: LOW
- **Dimension**: Sub-Record Byte Accounting
- **Record / Sub-record**: `PERK` / per-entry `DATA` (type 0, Quest + Stage)
- **Location**: `crates/plugin/src/esm/records/misc/magic.rs:562`
- **Status**: NEW (introduced by #6652, PERK-DEC-003)
- **Description**: The branch is `if sub.data.len() >= 6 { u16 } else { u8 }`, a length-based gate with no `GameKind`. xEdit FNV (`wbDefinitionsFNV.pas:5839`) is `Quest Stage itU16` + `wbUnused(2)`, FO4 (FO4.pas:11475) and FO76 (FO76.pas:12531) are `itU16` with a 6-byte body, but TES5 (tes5.pas, `wbPerkDATADecider` quest struct) is `Quest Stage itU8` + `wbByteArray('Unused', 3)`, an 8-byte body. On Skyrim the u16 read therefore takes the first unused byte as the stage's high byte.
- **Evidence**: Skyrim.esm has 6 Quest-type PERK entries, all `DATA` = 8 B, and bytes 5..8 are zero in all 6. The decode is correct on vanilla; only a plugin whose unused bytes are non-zero would produce a wrong (>255) stage. Unit test `parse_perk_quest_stage_is_u16` covers FO4 only.
- **Impact**: none on shipped data.
- **Suggested fix**: gate the u16 read on `game != Skyrim` (cite the three xEdit lines in the comment) and add the Skyrim 8-byte fixture.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related decoders / other games' arms
- [ ] **LOCK_ORDER**: n/a unless a RwLock scope changes
- [ ] **TESTS**: A regression test pins this specific fix (real-data pin where an install exists)
