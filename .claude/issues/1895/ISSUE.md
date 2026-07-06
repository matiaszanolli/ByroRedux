# REN-2026-07-05-L04: svgf.rs module docstring stale on indirect-history format + temporal-only scope

**Issue**: #1895 · **Severity**: LOW · **Labels**: low, renderer, documentation
**Dimension**: Denoiser/Composite · **Filed from**: docs/audits/AUDIT_RENDERER_2026-07-05.md (rt-deep suite)
**Location**: crates/renderer/src/vulkan/svgf.rs (module docstring: "Resource layout" + "Phase 3 only" lines)

## Description
Two stale claims in the module header:
1. `indirect_history[frame]` described as RGBA16F, but INDIRECT_HIST_FORMAT is B10G11R11_UFLOAT_PACK32
   (the #275 50%-savings change).
2. "Phase 3 only implements the temporal accumulation pass" — the live pipeline now runs the
   à-trous spatial pass (svgf_atrous.comp), same Session-49 gap as #1893.

## Evidence
- svgf.rs:11 `indirect_history … RGBA16F`; :3 "Phase 3 only implements the temporal accumulation pass".
- svgf.rs:111 INDIRECT_HIST_FORMAT = B10G11R11_UFLOAT_PACK32.
- svgf.rs:114 MOMENTS_HIST_FORMAT = R16G16B16A16_SFLOAT — the moments RGBA16F line is correct;
  only the indirect-history format line is wrong.

## Suggested Fix
Change the indirect_history format note to B10G11R11 and update the "Phase 3 only" sentence to
include the à-trous spatial pass.

**Related**: #1893 (L02, same drift in shader-pipeline.md); #1872 (footprint accounting).
