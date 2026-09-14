# #4295: REN-2026-09-14-D11-01: ground-cover debug-point pipeline enables writes on albedo (attachment 5) but its fragment shader has no location-5 output

- **Labels**: medium,renderer,pipeline,shaders,terrain-exterior,bug
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4295
- **Filed from**: docs/audits/AUDIT_RENDERER_2026-09-14.md

Source: `docs/audits/AUDIT_RENDERER_2026-09-14.md` (renderer audit, HEAD `147d97c3`)

- **Severity**: MEDIUM
- **Dimension**: Pipeline/RenderPass
- **Location**: `crates/renderer/src/vulkan/groundcover.rs` (`GroundCoverPipeline::build_pipelines`)
- **Status**: NEW
- **Description**: `build_pipelines` builds the blade and debug-point pipelines in one loop with one shared blend-attachment array (write masks on 0, 5, 6, 7). `groundcover_blade.frag` writes {0, 5, 6, 7}; `groundcover_debug.frag` writes only {0, 6, 7}.
- **Evidence**: shared blend array in the `build_pipelines` loop; no `layout(location = 5)` output in `groundcover_debug.frag`.
- **Impact**: in the debug-points view the albedo attachment receives undefined values; composite's `indirect * albedo` speckles. Same class as #3977. Debug view only.
- **Related**: #3977.
- **Suggested Fix**: give the debug pipeline its own blend array with attachment 5 masked off (or add the output), and pin both ground-cover fragment shaders' output locations the way the water test does.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types / pipelines / spawn sites)
- [ ] **DROP**: If Vulkan objects change, teardown is still reverse-order correct
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **TESTS**: A regression test pins this specific fix
