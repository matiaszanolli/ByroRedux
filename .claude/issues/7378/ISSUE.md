# #7378 — REN-D4-2026-11-23-01: the attachment-7 write masks of the opaque pipelines and of the blend arms are not pinned by any test

Snapshot of the issue as filed; GitHub is authoritative for current state.

Filed from `docs/audits/AUDIT_RENDERER_2026-11-23.md` (finding REN-D4-01). Reported by reading: nothing here was run on a GPU.

- **Severity**: LOW (test-gap; the code is currently correct)
- **Dimension**: Pipeline/RenderPass
- **Location**: `triangle_pipeline_inner` (the inline `color_blend_attachment` array, slot 7 = `G|B|A`) and `blend_gbuffer_attachments` (slots 6/7 = `fsr_mask_max_attachment()` in both arms) in `crates/renderer/src/vulkan/pipeline.rs`; the test `glass_pipeline_keeps_mesh_id_writable_for_its_own_caustic_source_gate` only asserts slots 1-5.
- **Status**: NEW (dedup: issues.json and `gh` search "far sun transparency attachment write mask test" return nothing; not in the 2026-11-16 report)
- **Description**: Attachment 7 (`outTransparencyAndSun`, `TRANSPARENCY_SUN_FORMAT` RGBA16F) has split channel ownership: R is the FSR transparency mask (blended draws MAX it in), GBA is the opaque receiver's far-field sun radiance (only opaque draws write it). The contract lives in three places: opaque pipelines mask `G|B|A` (`triangle_pipeline_inner`, device-requiring so not unit-testable as written), blend/water/composite use `fsr_mask_max_attachment` (R only; that function is pinned by `fsr_mask_max_attachment_is_a_red_only_max_accumulation`, but nothing asserts that `blend_gbuffer_attachments` *uses* it at 6 and 7), and the blade pipeline via `draw_color_write_masks` (pinned by `draw_write_masks_match_each_fragment_shaders_outputs`). A change to the opaque mask (back to `empty()`, as it was before slot 7 carried the sun lane, or to `RGBA`) or to the blend arm's slot 7 compiles, passes the 1694 tests, and either drops the far-sun input (far-sun pass reads zero radiance, exterior loses far shadows) or clobbers the FSR transparency mask.
- **Evidence**: `fsr_mask_off` / `color_blend_none.color_write_mask(G | B | A)` literals inline in `triangle_pipeline_inner`; grep of the renderer for `ColorComponentFlags::G | vk::ColorComponentFlags::B` finds only that production line; the pure helper `blend_gbuffer_attachments` returns `[...; 8]` and is asserted for indices 1-5 only. Stale labels beside it: `blend_gbuffer_attachments` still comments `// 6 fsr_reactive`, `// 7 fsr_transparency`; `water.rs` and `pipeline.rs` headers say "the two FSR masks" (slot 7 is now mask + sun).
- **Impact**: none today; a silent regression class (far-sun term or FSR mask) that only RenderDoc/visual review would catch.
- **Related**: #6964 (`fsr_mask_max_attachment`), Needs-RenderDoc N2.
- **Suggested Fix**: extract the opaque array into a pure `opaque_gbuffer_attachments()` beside `blend_gbuffer_attachments` (no behaviour change) and assert slot 7 = `G|B|A`, slot 6 = empty, and `[6]`/`[7]` of both blend arms equal `fsr_mask_max_attachment()`; refresh the stale slot-7 comments.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other passes, other shaders, other call sites)
- [ ] **TESTS**: A regression test pins this specific fix
