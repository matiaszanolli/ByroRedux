# ESM-2026-10-18-EXT-11: docs spliced by the `esm/cell` split: `WorldspaceRecord` docs sit on `worldspace_flags`, and the DATA flag table is incomplete

Labels: low,esm-plugin,documentation,doc-rot
Source: docs/audits/AUDIT_ESM_2026-10-18.md

- **Severity**: LOW
- **Dimension**: CELL / WRLD (docs)
- **Location**: `crates/plugin/src/esm/cell/worldspace.rs:6-128`
- **Status**: NEW (`d7c47d741`, #5805). #5965 re-pointed other split docs but not this one.
- **Description**: the `WorldspaceRecord` doc block (`:8-94`) now precedes `pub mod worldspace_flags`, so rustdoc attaches it to the module. The last sentence is cut in half: `:93-94` ends "not ahead of" / "The two [`WorldspaceRecord::flags`] bit layouts (#3506).", and its remainder, `/// one. See exal.md §5.`, is the only doc on the struct (`:128`). Separately, `worldspace_flags::tes4` leaves `0x20`, `0x40`, `0x80` unnamed, while xEdit FNV names them No LOD Noise, Don't Allow NPC Fall Damage and Needs Water Adjustment (`wbDefinitionsFNV.pas:8981`).
- **Suggested Fix**: move the first doc block back onto the struct and add the three FNV constants.

_Filed from `docs/audits/AUDIT_ESM_2026-10-18.md` (`/audit-suite --area exterior`, HEAD 6f44874ac). Fix with `/fix-issue <number>`._
