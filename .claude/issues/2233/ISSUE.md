# REN-D8-02: composite.frag's is_sky branch skips bloom and the volumetric/height-fog term

Severity: medium
Source audit: docs/audits/AUDIT_RENDERER_2026-08-02.md
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2233

**Dimension**: 8 (Composite) and 16 (Volumetrics) — cross-cutting, same root branch
**Location**: `crates/renderer/shaders/composite.frag` (`is_sky` branch, line 394-397; bloom and volumetric-fog application only in the `has_surface` branch, line 480+)
**Status**: NEW

**Description**: `has_surface = depth < 1.0` and `is_sky = !has_surface && (params.depth_params.x > 0.5)` split the composite shader into two branches. Both bloom and the volumetric/height-fog term are applied only inside the `has_surface` branch — sky pixels get neither. Two independent audit dimensions (8: composite/denoiser, 16: volumetrics) found the same root cause from different symptoms (REN-D8-02 for bloom, REN-D16-02 for the volumetric term); this issue tracks both since the report's own Prioritized Fix Order groups them as one `composite.frag` restructure.

**Evidence**: `composite.frag:394-397` (`is_sky` definition); the bloom-add and volumetric-fog-add terms are both gated inside `if (has_surface) Ellipsis` at line 480+, with no equivalent contribution on the `is_sky` path.

**Impact**: Sky pixels never receive bloom (bright sun/sky highlights don't bloom) or the volumetric fog/height-fog contribution (fog doesn't visually extend into the sky, producing a visible seam at the horizon/geometry silhouette).

**Suggested Fix**: restructure `composite.frag` so bloom and the volumetric fog term are computed once and applied to both branches (or move the `is_sky` early-return later, after those terms are folded in).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
