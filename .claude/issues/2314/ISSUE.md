# TD3-206: shader-pipeline.md's volumetrics descriptor-set description is a stale 2026-05-era snapshot

**Severity**: LOW
**Dimension**: 3 (Stale Documentation & Comments)
**Location**: `docs/engine/shader-pipeline.md:357-359`
**Labels**: low, renderer, tech-debt, documentation
**Source**: `docs/audits/AUDIT_TECH-DEBT_2026-08-03.md`

## Description
The doc describes the volumetrics private descriptor set as "froxel image,
VolumetricsParams/IntegrationParams UBO, TLAS." The live
`volumetrics_inject.comp` has 12 bindings (0-11), including `GpuFogVolume`'s
3 SSBOs (the struct that just received its lockstep test this window, via
`3f87a865`/#2228/#2231), light/cluster buffers, and density-noise samplers —
none of which are documented. No code-level drift (the lockstep tests are
green); this is a pure documentation-completeness gap, not a correctness
issue.

## Evidence
`docs/engine/shader-pipeline.md:357-359` vs. the live
`layout(set = ..., binding = 0..11)` declarations in `volumetrics_inject.comp`.

## Suggested Fix
Expand the binding-list prose to enumerate all 12 current bindings, or
replace with a reference to the GLSL source as the single source of truth
for binding indices.

## Age / Effort
Dates to the original 2026-05 volumetrics landing; widened by this window's
#2228/#2231 addition without a doc update. Effort: small.
