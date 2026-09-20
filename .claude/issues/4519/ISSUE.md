# REN-D2-2026-09-20-01: DBG_BYPASS_DETAIL is honored only on the primary detail combine — rayHitAlbedo takes no flags, so RT debug A/Bs keep the detail term

- **ID**: REN-D2-2026-09-20-01
- **Labels**: low,renderer,shaders,bug
- **Filed from**: docs/audits/AUDIT_RENDERER_2026-09-20.md

**Severity**: LOW · **Dimension**: Ray Queries
**Source**: docs/audits/AUDIT_RENDERER_2026-09-20.md (REN-D2-2026-09-20-01)

**Location**: `crates/renderer/shaders/triangle.frag:1429` (primary combine, honors the flag) vs `crates/renderer/shaders/include/ray_hit.glsl:533` (`rayHitAlbedo`, flag-less)

**Description**
The 0x2 DBG bypass works on the direct/raster detail combine only; reflections/GI/refraction albedo keeps the detail term during a debug A/B, so the diff understates the detail contribution. Previously unnumbered prose in the 2026-09-16 report.

**Evidence**
Source-shape verification during the 2026-09-20 audit (D2).

**Impact**
Diagnostic asymmetry only — a detail-map A/B via the debug bit reads partial.

**Suggested Fix**
Thread the flag (or the pre-combined albedo) into rayHitAlbedo, or document the bypass as primary-combine-only at the flag's definition.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
