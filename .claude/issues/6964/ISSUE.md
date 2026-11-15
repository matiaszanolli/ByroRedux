# #6964: TD2-2026-11-15-11: The FSR2/3 "reactive-mask MAX" blend attachment state is spelled three times

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/6964
- **Labels**: low,renderer,pipeline,tech-debt,bug
- **Source**: docs/audits/AUDIT_TECH_DEBT_2026-11-15.md

From the tech-debt audit `docs/audits/AUDIT_TECH_DEBT_2026-11-15.md` (HEAD 995ed089b).

- **Dimension**: 2 · **Severity**: LOW · **Effort**: trivial · **Labels**: `tech-debt` · **Existing**: none (the sibling `auxiliary_blend_attachment` was shared for the same reason in #3821)
- **Age**: composite.rs 5d3625416 (2026-07-28); pipeline.rs and water.rs similar
- **Location**: `crates/renderer/src/vulkan/pipeline.rs:693`, `water.rs:1010` (both named `fsr_mask_max`), `composite.rs:1120` (`mask_blend`); the shared sibling is `pipeline::auxiliary_blend_attachment` (`pipeline.rs:666`)
- **Description**: all three are `color_write_mask(R)`, `blend_enable(true)`, ONE/ONE factors, `BlendOp::MAX` on both colour and alpha. `water.rs` reuses `auxiliary_blend_attachment()` two lines above its copy with the comment "Shared ... rather than duplicated so the two pipelines' coverage semantics cannot drift apart", then spells the mask state inline. `pipeline.rs:690` carries the AMD integration-contract rationale for MAX accumulation, which the other two copies do not.
- **Suggested Fix**: `pub(crate) fn fsr_mask_max_attachment() -> vk::PipelineColorBlendAttachmentState` beside `auxiliary_blend_attachment`, with the rationale comment and a pin test like `auxiliary_blend_attachment_is_a_src_alpha_coverage_blend` (`pipeline.rs:1189`); the three call it.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or an extended guard) pins this specific fix
