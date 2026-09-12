# FO3-D1-2026-09-11-03: three FO3 shader-flag bits decode with no sink (External_Emittance, F2 Wireframe, F2 Premult_Alpha)

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4239
**Labels**: bug, nif-parser, low, legacy-compat, game:fo3
**Source**: `/audit-fo3` — `docs/audits/AUDIT_FO3_2026-09-11.md`, finding FO3-D1-2026-09-11-03

**Severity**: LOW
**Dimension**: FO3 Rendering Path (Inline Shaders) — `/audit-fo3` Dimension 1
**Location**: `crates/nif/src/shader_flags.rs:33-84` (`External_Emittance`, F2 `Wireframe`, F2 `Premult_Alpha` constants), `crates/nif/src/import/material/legacy_properties.rs:386-670` (no consumer)

**Description**: Three FO3 shader-flag bits decode with no sink: `External_Emittance`, F2 `Wireframe`, and F2 `Premult_Alpha`. All three are defined as constants in `shader_flags.rs` but are never read anywhere in `legacy_properties.rs` or `material_translate.rs`.

**Evidence**: `EXTERNAL_EMITTANCE` (`shader_flags.rs:207`), `WIREFRAME` (`:249`), `PREMULT_ALPHA` (`:252`) are defined; no reference to any of the three appears in `legacy_properties.rs` (the file that consumes every other decoded shader flag) or in `material_translate.rs`.

**Impact**: Small, bounded — alternative sources exist for wireframe/premult-alpha; `External_Emittance` affects a handful of cell-lit emissive surfaces.

**Suggested Fix**: No action required immediately — documented for the next auditor / contributor working on emissive or alpha-blend fidelity to wire these in if they become visually relevant.

## Completeness Checks
- [ ] **TESTS**: N/A until a consumer is added.
