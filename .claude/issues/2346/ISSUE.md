# OBL-D5-01: emissive_source_tests.rs module doc points at pre-#2059-split walker.rs line numbers

**Issue**: https://github.com/matiaszanolli/ByroRedux/issues/2346
**Severity**: LOW (documentation/audit-navigation only)
**Dimension**: NIFAL Canonical Material Translation for Oblivion
**Location**: `crates/nif/src/import/material/emissive_source_tests.rs:1-12`
**Source audit**: `docs/audits/AUDIT_OBLIVION_2026-08-03.md` (finding OBL-D5-01)
**Labels**: low, nif-parser, documentation

### Description
The module header cites `walker.rs:~292`/`~347`/`~578` for the three
`EmissiveSource` set-sites. Post-#2059, `walker.rs` is a 157-line orchestrator
containing none of those arms — BSLighting/BSEffect moved to
`dedicated_shader.rs`, `NiMaterialProperty` to
`legacy_properties.rs:89-108` (`apply_material_property`).

### Impact
Audit/navigation friction only. No runtime effect.

### Suggested Fix
Repoint the header table to
`dedicated_shader.rs::apply_dedicated_shader_property` and
`legacy_properties.rs::apply_material_property` by function name (survives
future file splits), not line number.
