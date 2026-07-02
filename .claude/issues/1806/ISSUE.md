# D2-NEW-05: draw_sort_key omits the wireframe pipeline axis — sort key and PipelineKey no longer in lockstep

**Issue**: #1806
**Labels**: low,renderer,pipeline,performance,bug
**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-07-02.md` (D2-NEW-05)

**Severity**: LOW
**Source**: `AUDIT_PERFORMANCE_2026-07-02.md` (D2-NEW-05)

## Location
`byroredux/src/render/mod.rs:192-240`; `crates/renderer/src/vulkan/pipeline.rs:114,121` (`Opaque { wireframe }`, `Blended { …, wireframe }`)

## Description
#869 added `wireframe` to `PipelineKey`, making it a batch-merge and pipeline-bind boundary, but `draw_sort_key` (a 10-tuple in both the alpha and opaque branches) was never extended with a matching slot — neither branch references `cmd.wireframe`. A wireframe draw interleaved among fill draws lands mid-run, splitting the instanced batch and forcing extra pipeline binds.

## Evidence
`pipeline.rs:114,121` `wireframe: bool` fields on both `PipelineKey` variants; `mod.rs:192-240`'s `draw_sort_key` tuple has no matching slot.

## Impact
Near-zero on shipped content (`NiWireframeProperty` is essentially absent from real assets) — a lockstep/hardening gap that becomes real if wireframe content (debug modes, mods) ever coexists with fill geometry.

## Related
#869, #1581.

## Suggested Fix
Fold `wireframe` into an existing u8 slot (e.g. pack with `two_sided`), and extend the sort-key/merge-axis lockstep test.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other hot-path loops / other dirty gates)
- [ ] **TESTS**: A regression test pins this specific fix

