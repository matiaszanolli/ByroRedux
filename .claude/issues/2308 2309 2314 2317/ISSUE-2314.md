title:	TD3-206: shader-pipeline.md's volumetrics descriptor-set description is a stale 2026-05-era snapshot
state:	OPEN
author:	matiaszanolli (Matias Zanolli)
labels:	documentation, low, renderer, tech-debt
comments:	0
assignees:	
projects:	
milestone:	
issue-type:	
parent:	
sub-issues:	
sub-issues-completed:	
blocked-by:	
blocking:	
number:	2314
--
## Description
`docs/engine/shader-pipeline.md:357-359` describes the volumetrics private descriptor set as "froxel image, VolumetricsParams/IntegrationParams UBO, TLAS." The live `volumetrics_inject.comp` has 12 bindings (0-11), including `GpuFogVolume`'s 3 SSBOs (the struct that just received its lockstep test this window, via `3f87a865`/#2228/#2231), light/cluster buffers, and density-noise samplers — none of which are documented. No code-level drift (the lockstep tests are green); this is a pure documentation-completeness gap, not a correctness issue.

## Evidence
`docs/engine/shader-pipeline.md:357-359` vs. the live `layout(set = 0, binding = 0..11)` declarations in `crates/renderer/shaders/volumetrics_inject.comp` (froxel image, VolumetricsParams UBO, TLAS, LightBuffer, ClusterGrid, ClusterLightIndices, previousFroxel, FogVolumeBuffer, FogClusterBuffer, FogClusterIndexBuffer, baseDensityNoise, detailDensityNoise).

## Suggested Fix
Expand the binding-list prose in `docs/engine/shader-pipeline.md:357-359` to enumerate all 12 current bindings, or replace with a reference to the GLSL source as the single source of truth for binding indices.

## Age
Dates to the original 2026-05 volumetrics landing; widened by this window's #2228/#2231 addition without a doc update.

## Completeness Checks
- [ ] **SIBLING**: Check other per-pass private-set descriptions in shader-pipeline.md (SVGF, TAA, bloom, composite, SSAO) for the same drift
- [ ] **TESTS**: Doc-only fix; not applicable — binding layout already pinned by the GLSL lockstep tests
