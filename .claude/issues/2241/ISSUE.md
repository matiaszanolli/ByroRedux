# REN-D16-03: Rectangle-rule slab integration lets dense local fog volumes over-brighten without bound

Severity: medium
Source audit: docs/audits/AUDIT_RENDERER_2026-08-02.md
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2241

**Dimension**: 16 (Volumetrics)
**Location**: `crates/renderer/shaders/volumetrics_inject.comp` (froxel slab integration around the `authored_inscatter`/`scattering_coef` accumulation, line ~440-460)
**Status**: NEW

**Description**: The per-slab in-scattering integration uses a rectangle-rule (single-sample-per-slab) approximation rather than the exact exponential-transmittance integral. For a dense local fog volume whose extinction coefficient is large relative to the froxel slab thickness, this approximation over-estimates in-scattered radiance without an upper bound, rather than converging to the correct saturated (fully-opaque medium) result.

**Impact**: A sufficiently dense authored local fog volume (e.g. thick smoke) can render visibly over-bright rather than saturating to its correct opaque-medium appearance.

**Suggested Fix**: replace the rectangle-rule per-slab estimate with the closed-form exponential slab integral (`L_slab = L_medium * (1 - exp(-sigma_t * dt))`), which is already used elsewhere in this same shader for the global medium (per the `equilibrium radiance` comment).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
