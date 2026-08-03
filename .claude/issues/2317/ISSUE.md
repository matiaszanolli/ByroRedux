# FO3-D1-02: FO3 parallax POM enabled by texture-slot presence, never by authored flags; height scale un-converted

Filed from: `docs/audits/AUDIT_FO3_2026-08-03.md`
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2317

**Severity**: MEDIUM
**Location**: `crates/nif/src/import/material/legacy_properties.rs:265-302`; `crates/nif/src/blocks/shader.rs:90-97`; `crates/renderer/shaders/triangle.frag:186-198`; `crates/renderer/shaders/include/material_sampling.glsl:18-23,74`
**Status**: NEW

### Description
Three compounding gaps, confirmed against current code — (a) no gate on bits 11 (`Parallax_Shader_Index_15`)/28 (`Parallax_Occulsion`), POM driven purely by texture-slot-3 presence (`legacy_properties.rs:267-269` reads `tex_set.textures.get(3)` unconditionally into `parallax_map`); (b) `parallax_max_passes`/`parallax_height_scale` written unconditionally (`legacy_properties.rs:293-299`, gated only on `.is_none()`, not on flag bits), unlike the sibling `NiTexturingProperty` branch which is gated + uses engine defaults; (c) no unit conversion — the shader's own contract says `heightScale` is typically 0.02–0.08, but the FO3 `bsver<=24` fallback delivers `parallax_scale = 1.0` (`blocks/shader.rs:90-97`, `(4.0, 1.0)` default tuple), ~25× mismatch, producing texture swimming at grazing angles.

### Impact
FO3 parallax architecture (Pitt/Point Lookout/Megaton brick+concrete) gets either texture swimming or unauthored POM.

### Suggested Fix
Add the two flag bits to `fo3nv_f1`, gate the parallax writes on them + `parallax_map.is_some()`, and define the FO3 Parallax Scale → engine `heightScale` conversion at the import boundary.

### Related
#453, #452, #725/NIF-D4-06, FO3-D1-01

## Completeness Checks
- [ ] **SIBLING**: Shared code path with FNV/Oblivion — same POM gating gap applies there
- [ ] **CANONICAL-BOUNDARY**: Unit conversion (FO3 Parallax Scale → engine heightScale) belongs at the NIFAL parser→`Material` boundary, not in the shader. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins "FO3 mesh without Parallax flag bits ⇒ no POM; height scale converted to shader's 0.02–0.08 contract range"
