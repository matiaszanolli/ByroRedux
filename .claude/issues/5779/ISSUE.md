# OBL-D2-2026-10-14-02: CELL `XCCM` has no Oblivion-keyed pin; the Oblivion arm is correct only by `matches!(Skyrim | Fallout4)` omission

**Labels**: low,game:oblivion,legacy-compat,esm-plugin,test-gap,bug
**Source**: docs/audits/AUDIT_OBLIVION_2026-10-14.md

Found by `/audit-oblivion` (docs/audits/AUDIT_OBLIVION_2026-10-14.md, HEAD `adbd468e2`). Finding ID: `OBL-D2-2026-10-14-02`. Re-verified against current code before filing.

- **Severity**: LOW (test gap, behaviour correct)
- **Dimension**: 2 (`walkers.rs` / `cell/helpers.rs`)
- **Location**: `xccm_targets_region`, `CellSubrecordFields::absorb` (`crates/plugin/src/esm/cell/helpers.rs`); tests in `crates/plugin/src/esm/cell/tests/cell.rs`
- **Status**: NEW
- **Description**: #5463 splits XCCM by game. Oblivion falls into the CLMT arm (`climate_override`), which is right: `Oblivion.esm` authors 55 CELL XCCM and all 55 resolve to CLMT records (8 distinct climates, e.g. `0x32E16` ×26, `0xC099A` ×12). The new tests cover FO3/FNV (CLMT) and Skyrim/FO4 (REGN) only; the FO3 test stands in for TES4.
- **Evidence**: `xccm_targets_region(game) = matches!(game, Skyrim | Fallout4)`; the XCCM tests name only `Fallout3NV`, `Skyrim`, `Fallout4`.
- **Impact**: none today. A future edit that adds `Oblivion` to the region arm, or a `GameKind` refactor, passes every current pin.
- **Related**: #5463 (closed), #693.
- **Suggested Fix**: add `GameKind::Oblivion` to the CLMT-side test loop, and optionally an `--ignored` `Oblivion.esm` census that every XCCM target is a CLMT.

### Dim 1 — NIF version handling & corpus integrity

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other games' arms, sibling parsers/tests)
- [ ] **TESTS**: A regression test pins this specific fix
