# 2175: D2-04: Water pass rebinds pipeline + 3 descriptor sets once per water plane instead of once per pass

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/2175
**Labels**: bug, low, performance

---

## Severity
LOW

## Dimension
Draw-Call & Instancing Efficiency (Dim 2) — `/audit-performance` 2026-07-25

## Location
`crates/renderer/src/vulkan/water.rs` (per-plane draw loop)

## Description
The water render pass rebinds its pipeline and 3 descriptor sets on every distinct water plane in the current scene, rather than binding once per pass and iterating planes with only the per-instance push-constant/UBO updated in between.

## Impact
Extra `vkCmdBindPipeline`/`vkCmdBindDescriptorSets` calls per water plane. Cell-dependent blast radius (most interiors have 0-1 water planes; some exteriors have several), so cost is bounded but avoidable.

## Related
None — standalone finding.

## Suggested Fix
Hoist the pipeline/descriptor-set bind out of the per-plane loop; bind once per pass and iterate planes with per-instance data only.

## Completeness Checks
- [ ] **TESTS**: Existing water-pass tests should verify unchanged visual output after hoisting the bind calls
