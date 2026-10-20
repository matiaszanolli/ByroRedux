# TD1-2026-10-19-09: `byroredux/src/cell_loader.rs` is a 36-`pub use` facade that also owns two unrelated jobs

Labels: low,tech-debt,bug
Source: docs/audits/AUDIT_TECH_DEBT_2026-10-19.md

- **Severity**: LOW
- **Dimension**: 1 - File / Function / Module Complexity (mod.rs/lib.rs with > 20 `pub use`)
- **Location**: `byroredux/src/cell_loader.rs:229` `pack_effect_shader_flags`, `:279` `pack_imported_material_flags` (~130 lines, 34 call sites outside the file), `:637` `note_player_entered_cell` (new, M47.3 respawn clock hook)
- **Status**: NEW (35 `pub use` at base; the M47.3 hook is new)
- **Effort**: trivial
- **Description**: the module doc promises a split entry-point facade; the material-flag packers (#890 Stage 2) and a gameplay respawn hook are neither. Other `pub use` hubs are clean: `scripting/lib.rs` 67 (pure facade, `register` moved out by #5806), `core/ecs/components/mod.rs` 43, `byroredux/src/systems.rs` 28, `plugin/esm/records/mod.rs` 25 contain no logic.
- **Proposed fix**: `cell_loader/material_flags.rs` (the two packers, re-exported from `cell_loader.rs`) and `note_player_entered_cell` into `cell_loader/load.rs` or the M47.3 caller's module; no caller edits.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the related files named above
- [ ] **TESTS**: A regression test / source-scan guard pins this specific fix (or the finding is a pure move/delete covered by existing tests)

_Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-19.md` (HEAD 12677d718). Fix with `/fix-issue <number>`._
