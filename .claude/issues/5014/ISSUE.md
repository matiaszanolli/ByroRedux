# PAR-D5-2026-09-29-02: FaceGen docs still describe the pre-#4653/#4668 layouts in four places

**Labels**: low,documentation,doc-rot,import-pipeline

**Source report**: `docs/audits/AUDIT_PARSERS_2026-09-29.md`

- **Severity**: LOW
- **Dimension**: Decode-Consumer Wiring
- **Location**: `crates/facegen/src/egm.rs:65-66`, `crates/facegen/src/egt.rs:15-22`, `crates/facegen/src/tri.rs:33-34`
- **Status**: NEW
- **Trigger Input**: n/a (documentation).
- **Description**:
  - **EGM.** The `EgmMorph::scale` doc says it "multiplies the f16 delta". Deltas have been int16 since #4653.
  - **EGT header size.** The header block adds up to 8 + 5×u32 + `padding: [u8; 32]` = 60 bytes. `HEADER_BYTES` is 64, and both the parser and the test synth treat bytes 28..64 (36 bytes) as padding.
  - **EGT R/C labels.** The block labels `R` "image rows (width)" and `C` "image columns (height)", but a row count is a height. Square vanilla images hide this. It would transpose a non-square mod EGT once a compositor exists.
  - **TRI.** The module doc says "24 further bytes" follow the ten header words. 8 + 40 = 48, so 16 bytes remain, which is what the test synth pads.
- **Evidence**: see the locations.
- **Impact**: misleading format documentation for the future EGT compositor and `.tri` lip-sync work (both have no consumer yet, #3544).
- **Related**: #4653, #4668, #3544
- **Suggested Fix**: correct the four doc sites, and name the EGT fields per the SDK (C columns = width, R rows = height).

**Validated at HEAD 9fcfdc3fc**: `crates/facegen/src/egm.rs` `EgmMorph::scale` doc still says "multiplies the f16 delta"; `egt.rs` header block still lists `padding: [u8; 32]` against `HEADER_BYTES = 64` and labels R as width / C as height; `tri.rs` module doc still says "24 further bytes".

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
