# PERF-D5-NEW-03: SVGF a-trous recomputes the 5x5 spatial-variance estimate in all 5 iterations

**Issue**: #1813
**Labels**: low,renderer,pipeline,performance,bug
**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-07-02.md` (PERF-D5-NEW-03)

**Severity**: LOW
**Source**: `AUDIT_PERFORMANCE_2026-07-02.md` (PERF-D5-NEW-03)

## Location
`crates/renderer/shaders/svgf_atrous.comp:134-150` (unconditional 5x5 luminance loop); dispatch loop `crates/renderer/src/vulkan/svgf.rs:88` (`ATROUS_ITERATIONS = 5`), `:1258-1288`

## Description
Each of the 5 a-trous iterations re-derives a 5x5 local luminance variance (`spatialVar`, `svgf_atrous.comp:134-150`, no iteration-index gate) and re-runs the 3x3 temporal-variance prefilter (`:108-123`) before the 25-tap edge-stopped filter. The spatial estimate is a legitimate iteration-0 concern (catches converged-but-noisy pixels), but iterations 1-4 run it against already-filtered color whose local variance shrinks monotonically, mostly duplicating work with diminishing contribution. (Distinct from the shader's own header note about not filtering variance alongside color — that is a separate deferred refinement.)

## Evidence
`svgf_atrous.comp:134-150` `spatialVar` computed unconditionally, no gate on iteration index; `svgf.rs:88` `ATROUS_ITERATIONS = 5` dispatches the same shader 5x per frame.

## Impact
Constant-factor bandwidth/ALU on 5 full-screen dispatches (~460M extra, heavily L2-cached, texel fetches/frame at 1440p); the pass remains strictly O(pixels). Confidence: HIGH on the cost; the safety of computing spatial-variance once and propagating it needs a visual A/B against the dark-floor moiré regression scene before shipping.

## Related
#1662 / Session 49 denoiser overhaul.

## Suggested Fix
Compute the spatial-variance estimate in iteration 0 only, propagate through the unused moments-image channel, falling back to temporal-variance-only weight in later iterations.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader passes / other reservoir arrays)
- [ ] **TESTS**: A regression test pins this specific fix

