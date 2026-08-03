# FO3-D1-05/D2-01: FO3/FNV fire-refraction heat-haze never classified — refraction fields decoded then dropped at NIFAL boundary

Filed from: `docs/audits/AUDIT_FO3_2026-08-03.md`
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2321

**Severity**: MEDIUM
**Location**: `crates/nif/src/import/material/legacy_properties.rs:237-336` (`apply_pp_lighting_property`, no refraction mirroring); decode site `crates/nif/src/blocks/shader.rs:80-88`; the Skyrim-only classifier at `crates/nif/src/import/material/dedicated_shader.rs:307,330-338`; missing constants at `crates/nif/src/shader_flags.rs:33-44`
**Status**: NEW (merged from FO3-D1-05 / FO3-D2-01 — found independently by both Dimension 1 and Dimension 2)

### Description
`BSShaderPPLightingProperty::parse` correctly decodes `refraction_strength`(f32)/`refraction_fire_period`(i32) for all FO3 content (bsver 34 > the `#BSVER# #GT# 14` gate). Neither value is ever mirrored into `MaterialInfo` — the only writer of `MaterialInfo::refraction_strength` is the Skyrim+ `BSLightingShaderProperty` path, and the only site that sets `material_kind = 103` (`MATERIAL_KIND_FIRE_REFRACTION`) tests Skyrim-era `skyrim_slsf1` bits that have no FO3/FNV equivalent declared in `fo3nv_f1`.

Confirmed against current code: `crates/nif/src/import/material/dedicated_shader.rs:307` (`info.refraction_strength = shader.refraction_strength;`) and `:336` (`info.material_kind = 103;`) are both inside the Skyrim+ dedicated-shader path. A grep of `legacy_properties.rs` (the FO3/FNV path) finds no write to `info.refraction_strength` anywhere, despite `blocks/shader.rs:83,118-119` decoding `refraction_strength`/`refraction_fire_period` for the legacy `BSShaderPPLightingProperty` struct. Corroborating evidence: nif.xml declares two FO3-exclusive controllers (`BSRefractionStrengthController`, `BSRefractionFirePeriodController`) whose entire purpose is animating these fields, both already dispatched by this codebase.

### Impact
FO3/FNV fire, explosion, and plasma heat-haze proxies render as flat opaque lit slabs, potentially occluding flame cards authored behind them. Shared FO3+FNV blast radius. Kept at MEDIUM rather than HIGH because it's not yet quantified how many *vanilla* FO3 meshes actually author the `Refraction | Fire_Refraction` bit pair without adding new counting tooling (the one name-matching vanilla asset checked, `testrefractcloud01.nif`, is a dev test cloud) — escalate to HIGH if a bit-pair census against `meshes\fire\*` shows it's authored on shipped content.

### Suggested Fix
Add `REFRACTION`/`FIRE_REFRACTION` (bits 15/16) to `fo3nv_f1`, mirror `shader.refraction_strength` into `info.refraction_strength` in `apply_pp_lighting_property`, gate `material_kind = 103` on the FO3/FNV flag pair matching the Skyrim path's promotion. First step should be the bit-pair census to settle severity.

### Related
#2232 (REN-D6-01, the `ior` field overload), #2249, #452/#773, #2297 (MAT-D1-NEW-02, TLAS mask exclusion for MATERIAL_KIND_FIRE_REFRACTION — adjacent but distinct bug on the render side)

## Completeness Checks
- [ ] **SIBLING**: Shared code path with FNV — same fix applies there; cross-reference #2232/#2297 which touch the same `material_kind = 103` fire-refraction path from the render side
- [ ] **CANONICAL-BOUNDARY**: Refraction mirroring belongs at the NIFAL parser→`Material` boundary (`legacy_properties.rs` → `MaterialInfo`), matching the existing Skyrim+ path in `dedicated_shader.rs`. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins "FO3 BSShaderPPLightingProperty with Refraction|Fire_Refraction flags ⇒ material_kind == 103 and refraction_strength mirrored"
