# FO3-D1-04: FO3 BSShaderProperty.shader_type is parsed and never consumed — Skin/Water/Sky/Tile/Lighting30 indistinguishable from Default

Filed from: `docs/audits/AUDIT_FO3_2026-08-03.md`
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2320

**Severity**: MEDIUM
**Location**: `crates/nif/src/blocks/base.rs:227-271` (parsed); `crates/nif/src/import/material/legacy_properties.rs:239-336` (never read); doc-rot at `crates/nif/src/import/material/mod.rs:421-426`
**Status**: NEW

### Description
No importer arm reads `shader.shader.shader_type` for the FO3 legacy path. `MaterialInfo.material_kind` stays 0 for every `BSShaderPPLightingProperty` mesh; the doc-comment attributing "shader_type = 3/7" parallax kinds to FO3 is actually describing Skyrim's `BSLightingShaderType` enum, not FO3's.

Confirmed against current code: `shader_type` is parsed at `blocks/base.rs:232,259` (`let shader_type = stream.read_u32_le()?;`) but a grep of `legacy_properties.rs` finds no site reading `shader.shader.shader_type` — only comments referencing "shader_type" in unrelated contexts. The doc comment at `import/material/mod.rs:421-426` on `parallax_map` still reads "architecture relies on this for brick-wall / concrete parallax-occlusion mapping on `shader_type = 3` ... and `shader_type = 7`" — those numeric values are Skyrim's `BSLightingShaderType` enum, not anything FO3's `BSShaderPPLightingProperty.shader_type` actually carries.

### Impact
FO3 `SHADER_SKIN`(14)/`SHADER_WATER`(17)/`SHADER_SKY`(10) materials all import as generic lit `material_kind=0`, blocking any future skin-SSS/sky-fullbright/water-dispatch branch. Root cause of FO3 water having no distinct GPU material kind.

### Suggested Fix
Capture `shader.shader.shader_type` into a new `MaterialInfo::legacy_shader_type` field (do not alias onto `material_kind`, whose range is the Skyrim enum), correct the doc.

### Related
#1856, #977

## Completeness Checks
- [ ] **SIBLING**: Same gap likely applies to FNV (shared code path)
- [ ] **CANONICAL-BOUNDARY**: New `legacy_shader_type` field belongs on `MaterialInfo` at the NIFAL parser→`Material` boundary, distinct from the Skyrim `material_kind` enum range. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins "FO3 shader_type is captured into a legacy-specific field, not conflated with Skyrim's material_kind enum"
