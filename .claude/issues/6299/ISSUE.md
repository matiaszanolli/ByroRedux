# #6299: ESM-2026-10-21-05: Two stale doc comments in the delta

**Labels**: low,documentation,doc-rot,esm-plugin

**Source**: docs/audits/AUDIT_ESM_2026-10-21.md · **Severity**: LOW · **Dimension**: FormID & Load Order / CELL / WRLD Walkers · **Location**: `crates/plugin/src/esm/records/parse.rs` (`parse_esm_with_load_order` doc); `crates/plugin/src/esm/cell/wrld.rs` (#6043 XCLC comment)

- **Location**: `crates/plugin/src/esm/records/parse.rs:100`; `crates/plugin/src/esm/cell/wrld.rs:436-439`
- **Status**: NEW
- **Description**:
  - (a) The `parse_esm_with_load_order` doc still tells callers to pass `Some(FormIdRemap { plugin_slot, master_slots })`. Since #6118 the struct has a third `scheme` field, and a Starfield caller that follows the doc gets the positional scheme, which is the regression #6118 fixed. The only production constructor (`load_order.rs:409`) is correct.
  - (b) The #6043 comment says "FO3/FNV's trailing dword is padding with a zero low byte". xEdit's common `wbCellGrid` (`wbDefinitionsCommon.pas:8690-8698`) gives FO3/FNV the same Land Flags byte. FalloutNV.esm authors a non-zero one (one cell, value `4` = Hide Quad 3). The unconditional read is correct; only the justification is wrong.
- **Suggested Fix**: point the `parse.rs` doc at `build_remap_for_plugin` (or name `scheme`), and reword the `wrld.rs` comment to cite `wbCellGrid` for every game.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
