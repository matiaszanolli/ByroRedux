# TD2-2026-10-15-14: The interior and exterior CELL walkers still repeat the 22-field `CellSubrecordFields` destructure and the 28-field `CellData` literal after #5309

Labels: low,esm-plugin,tech-debt,bug

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD2-2026-10-15-14`).

- **Severity**: LOW
- **Dimension**: 2 - Logic Duplication
- **Location**: `crates/plugin/src/esm/cell/walkers.rs:430-481` vs `crates/plugin/src/esm/cell/wrld.rs:430-498`
- **Status**: NEW (residue of closed #5309 `78c2b2d61`, which removed the 20 duplicated arms but not the consumption)
- **Age**: `78c2b2d61` (2026-10-05)
- **Effort**: trivial
- **Description**: both walkers destructure the identical `CellSubrecordFields { editor_id, display_name, .. }` then rebuild a `CellData` field by field; they differ in `is_interior`, `show_sky`, `grid`, `lighting`. `precombined_mesh_hashes` / `absorbed_ref_bakes` / `regional_color_override` were each added to both by hand.
- **Evidence**: clone scan 20 windows `walkers.rs:195-478` / `wrld.rs:383-476`.
- **Impact**: a new CELL sub-record (an XCLL-like field, the next `XCRI`-style hash) is three edits (struct, absorb arm, two literals).
- **Related**: #5309, #5484.
- **Suggested Fix**: `impl CellSubrecordFields { fn into_cell(self, form_id: u32, is_interior: bool, show_sky: Option<bool>, grid: Option<(i32,i32)>, lighting: Option<CellLighting>) -> CellData }` in `cell/helpers.rs`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
- [ ] **TESTS**: A regression test or guard pins this specific fix (or the existing guard is named)
