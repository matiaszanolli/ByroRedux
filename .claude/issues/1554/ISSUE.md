**Severity**: LOW · **Dimension**: Multi-Master Load Order + TES5 Cell-Load
**Location**: `crates/plugin/src/esm/reader.rs:256-297` (`FormIdRemap::remap`); `read_file_header` never reads the TES4 `0x0200` ESL flag
**Source**: AUDIT_SKYRIM_2026-06-14 (SK-D4-03)

## Description
`FormIdRemap::remap` treats the top byte of a FormID as a flat mod-index. ESL / light-master plugins use the `0xFE` prefix with a 12-bit sub-index (`0xFExxx`), so an ESL's FormIDs need a different decode. The file-header parser never reads the `0x0200` ESL header flag, so ESL plugins would remap incorrectly.

## Evidence
`reader.rs:256-297` is a flat top-byte index (`let mod_index = (raw >> 24) as u8;`), with no `0xFE`/12-bit-subindex branch; no `0x0200` flag read in `read_file_header`. Verified **none of the seven vanilla/DLC/CC Skyrim SE masters are ESL-flagged**, so the blast radius for vanilla Skyrim compat is zero — this is a forward-looking gap for third-party ESL mods (and FO4/SF, which ship ESL CC content).

## Impact
Third-party ESL mod FormIDs would resolve to wrong records. No impact on vanilla Skyrim SE.

## Suggested Fix
Read the `0x0200` header flag; for ESL-flagged plugins, decode FormIDs as `0xFE` + 12-bit load-order sub-index + 12-bit local ID per the Creation Engine spec.

## Completeness Checks
- [ ] **SIBLING**: Same decode applied wherever FormIDs are remapped (cell + record paths), not just `remap`
- [ ] **TESTS**: A regression test pins `0xFExxx` decode for an ESL-flagged plugin
