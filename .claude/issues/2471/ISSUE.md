# REN-D17-NEW-01: Kaplanyan-Hoffman specular AA filters alpha instead of alpha-squared -- under-filters exactly the smooth surfaces it exists for

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2471
**Finding ID**: REN-D17-NEW-01 (source: `docs/audits/AUDIT_RENDERER_2026-08-07.md`)

**Severity**: MEDIUM
**Dimension**: 17 — Disney BSDF
**Location**: `crates/renderer/shaders/include/pbr.glsl:specularAaRoughness` (lines 210-217)
**Status**: NEW

## Description
The published Kaplanyan & Hoffman 2016 filter (Filament `normalFiltering()`, §4.10.1) widens the GGX **α²** by the kernel variance: `α²_filtered = α² + 2σ²`. This shader's documented convention is `α = roughness²` (stated by the sibling helper `deriveAxAy` and confirmed by `distributionGGX`). `specularAaRoughness` instead computes `roughness2 = roughness * roughness` — which is **α, not α²** — adds `2 * kernelVariance` to that, and `sqrt`s back. The caller squares the return, so the effective result is `α_filtered = α + 2σ²` rather than `α_filtered = sqrt(α² + 2σ²)`.

## Evidence
```glsl
float specularAaRoughness(vec3 N, float roughness) {
    vec3 dNdx = dFdx(N); vec3 dNdy = dFdy(N);
    float kernelVariance = 0.25 * (dot(dNdx, dNdx) + dot(dNdy, dNdy));
    float roughness2 = roughness * roughness;      // == α, NOT α²
    float filteredR2 = clamp(roughness2 + 2.0 * kernelVariance, 0.025 * 0.025, 1.0);
    return sqrt(filteredR2);                       // caller squares → α + 2σ²
}
```
Call path: `triangle.frag:2269` and `lighting.glsl:119` → `distributionGGX(NdotH, aaRoughness)` / `deriveAxAy(aaRoughness, ...)`, both of which square the argument to get α. Numeric gap at perceptual roughness `p = 0.1` (α = 0.01), σ² = 1e-3: current form gives α_f = 0.012; published form gives `sqrt(1e-4 + 2e-3) = 0.0458` — roughly 4× narrower filtering than intended. The two forms only converge as `p → 1`.

## Impact
Specular aliasing is under-suppressed on low-roughness normal-mapped surfaces at distance — corrugated metal, brick mortar, fence cutouts, polished trim. The lobe is widened by a constant `2σ²` regardless of base roughness, so smooth surfaces (where aliasing is worst) get proportionally the least help. Blast radius: every raster fragment with `DBG_DISABLE_SPECULAR_AA` clear — both no-cluster fallback and clustered lighting; both isotropic and anisotropic NDF branches.

## Related
`deriveAxAy`'s "0.025 floor mirrors specularAaRoughness's filteredR² ≥ 0.025² clamp" comment inherits the same mis-scaling. `AUDIT_RENDERER_2026-05-07.md:39` marked this helper "verified correct" — that verification did not trace the α-vs-α² convention through the caller.

## Suggested Fix
Square once more before adding the variance and take the fourth root on the way out: `filteredA2 = clamp(roughness2*roughness2 + 2*kernelVariance, ...); return sqrt(sqrt(filteredA2));` — matching Filament's round-trip. Re-check the `0.025²` floor's meaning under the corrected units and recompile `triangle.frag.spv` + `water.frag.spv`.

## Completeness Checks
- [ ] **TESTS**: A regression test pins the numerically corrected filter at a known roughness/variance pair
- [ ] **SIBLING**: `deriveAxAy`'s 0.025 floor is re-checked under the corrected units
