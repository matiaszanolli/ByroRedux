# Issue #3826: REN-WD-D8-02: water hand-rolls HDR alpha instead of coverage_alpha_factors, overwriting accumulated transparent coverage

**Labels**: low,renderer,water,bug
**Filed**: 2026-09-04, via /audit-publish from the water-deep audit suite

---

**Severity**: LOW
**Dimension**: Denoiser/Composite
**Location**: `crates/renderer/src/vulkan/water.rs` (`hdr_blend` in `build_pipeline`), `crates/renderer/src/vulkan/pipeline.rs` (`coverage_alpha_factors`), `crates/renderer/shaders/composite.frag` (the `is_sky` arm's `float coverage = clamp(direct4.a, 0.0, 1.0);`)
**Source report**: `docs/audits/AUDIT_RENDERER_2026-09-04.md` (water-deep suite, Dim 8)

## Description
`coverage_alpha_factors` exists so the HDR attachment's alpha channel behaves as an **accumulated** coverage lane for sky-silhouetted transparents (#2466): for a classic `ONE_MINUS_SRC_ALPHA` blend it returns `(ONE, ONE_MINUS_SRC_ALPHA)`, the over-operator. The water pipeline does not call it — it hardcodes `.src_alpha_blend_factor(ONE).dst_alpha_blend_factor(ZERO)`, which **replaces** the destination coverage with the water fragment's own alpha. When water draws over an already-blended transparent that has nothing opaque behind it (ocean/LOD water at the horizon behind spray, fog cards or particles), composite's sky arm then computes `compute_sky(dir) * (1 - waterAlpha)` and re-admits sky the earlier layer had already covered.

## Evidence
`water.rs` `hdr_blend` builder chain (`src_alpha_blend_factor(ONE)`, `dst_alpha_blend_factor(ZERO)`) vs `pipeline.rs::coverage_alpha_factors`, whose doc comment states the lane's purpose and whose only callers are the opaque/blend pipeline paths.

## Impact
A brightness/haze seam where water overlaps another transparent against open sky. Single-layer water over sky is unaffected (replace and accumulate agree when the destination coverage is 0). Visual only.

## Related
#2466, #2920.

## Suggested Fix
Route water's HDR attachment alpha factors through `coverage_alpha_factors(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)` so all transparent writers share one coverage convention.

## Completeness Checks
- [ ] **TESTS**: Needs a capture-based check (water overlapping another transparent against open sky), not `cargo test`
