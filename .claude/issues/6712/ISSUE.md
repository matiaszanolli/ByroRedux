# RF7-2026-11-05-02: `material_translate` depends on `cell_loader` for flag packers and `asset_provider` for constants

Labels: low,tech-debt,bug,nifal

**Severity**: LOW (no runtime effect; maintainability)
**Dimension**: Refactor backlog, item 7
**Status**: NEW. Part of #6680

## Location
- `byroredux/src/material_translate.rs:783-785` uses `pack_effect_shader_flags` and `pack_imported_material_flags` (`byroredux/src/cell_loader/material_flags.rs:18`, `:68`)
- `byroredux/src/material_translate.rs:1415-1416` uses `NEAR_MIRROR_ROUGHNESS_FLOOR` and `NEAR_MIRROR_NEUTRAL_ROUGHNESS` (`byroredux/src/asset_provider/material/mod.rs:313-318`)

## Why
`material_translate` is the canonical translation boundary, yet two of its inputs live in modules above it. `systems` also reaches into `cell_loader` for `pack_effect_shader_flags`.

## Change
Move `material_flags.rs` and the two constants next to the translation code, and update the callers.

## Constraints
Move items verbatim. Run `grep -rn include_str` for each source file first. This does not propose splitting the binary crate. A test at `byroredux/src/material_translate.rs:4389` reads `crate::asset_provider::material::SOURCES`.

## Acceptance Criteria
- `material_translate` has no production `cell_loader` or `asset_provider` reference
- Translated material output is unchanged, shown by the existing translation tests and non-timing tokens of the `bench:` line match a baseline build of the parent commit

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the NIFAL parser-to-`Material` boundary, never in shaders or at render time
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A test or guard pins the change, and any guard that scans the old spelling was extended in the same commit

---
Source: refactor review of 2026-11-05 (at `16d9e18f3`), re-measured at `02f46f95d`. Counts come from line and regex scripts. Fix with: `/fix-issue <this issue>`.

_Immutable snapshot of the issue as filed. GitHub is authoritative for current state (`gh issue view 6712 --json state`)._
