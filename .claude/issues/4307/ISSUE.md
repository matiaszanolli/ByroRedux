# #4307: REN-2026-09-14-D11-03: surface-format change rebuilds triangle/water pipelines but not ground-cover pipelines

- **Labels**: low,renderer,pipeline,vulkan,terrain-exterior,bug
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4307
- **Filed from**: docs/audits/AUDIT_RENDERER_2026-09-14.md

Source: `docs/audits/AUDIT_RENDERER_2026-09-14.md` (renderer audit, HEAD `147d97c3`)

- **Severity**: LOW
- **Dimension**: Pipeline/RenderPass
- **Location**: `crates/renderer/src/vulkan/context/resize.rs` (`recreate_swapchain_core`)
- **Status**: NEW
- **Description**: on a surface-format change `recreate_swapchain_core` rebuilds the render pass and the triangle and water pipelines, but not the ground-cover pipelines.
- **Impact**: not a spec violation today because the new render pass is compatible; becomes VUID-vkCmdDraw-renderPass-02684 if a main-pass attachment ever depends on the swapchain format.
- **Suggested Fix**: rebuild ground-cover pipelines alongside the other main-pass pipelines, or assert render-pass compatibility at that site.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types / pipelines / spawn sites)
- [ ] **DROP**: If Vulkan objects change, teardown is still reverse-order correct
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **TESTS**: A regression test pins this specific fix
