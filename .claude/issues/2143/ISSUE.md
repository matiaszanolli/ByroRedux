# 2143: CONC-D1-2026-07-25-01: Presentation render pass suppresses its implicit outgoing dependency with dstStageMask = NONE

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/2143
**Labels**: bug, low, vulkan

---

## Severity
LOW

## Dimension
Vulkan Queue & AS Sync — `/audit-concurrency` 2026-07-25

## Location
`crates/renderer/src/vulkan/presentation.rs:173-178`

## Description
The presentation render pass declares an explicit `srcSubpass = 0 → dstSubpass = VK_SUBPASS_EXTERNAL` dependency with `dstStageMask = NONE` and no `dstAccessMask` — replacing Vulkan's implicit end-of-pass dependency with one whose second sync scope is empty, leaving the pass's `COLOR_ATTACHMENT_WRITE` unordered against any later command. Every sibling pass (composite, egui) declares a real dst scope; presentation is the only outlier.

## Evidence
`.dst_stage_mask(vk::PipelineStageFlags::NONE)` at `presentation.rs:173-178`, versus `composite.rs:547-555` (`COMPUTE_SHADER|TRANSFER`) and `egui_pass.rs:322-328` (`BOTTOM_OF_PIPE`, with an explicit "don't rely on the implicit edge" comment).

## Impact
No live hazard today — the two current downstream consumers (egui overlay, screenshot copy) each carry their own incoming barrier with a matching src scope, and the present itself is covered by the `render_finished` semaphore. The exposure is forward-looking: a future pass added between `presentation.dispatch` and `end_command_buffer` without its own barrier would race the swapchain image with nothing to catch it in `cargo test`.

## Trigger Conditions
Not reproducible today; requires a future code change adding an unbarriered swapchain-image consumer.

## Verification Path
`BYRO_VALIDATION=sync` on a screenshot-capture frame and an egui-overlay frame; absence of `SYNC-HAZARD-READ_AFTER_WRITE` on the swapchain image is the evidence the two self-synchronizing consumers are sufficient today.

## Related
`composite.rs:547-555` (the pattern to mirror), commit `33d6a18e`.

## Suggested Fix
Give the outgoing dependency a real dst scope mirroring the actual consumers (`COLOR_ATTACHMENT_OUTPUT | TRANSFER` / `COLOR_ATTACHMENT_READ|WRITE | TRANSFER_READ`), or delete the explicit dependency and let Vulkan synthesize the implicit one. Confirm with `BYRO_VALIDATION=sync` before/after.

## Completeness Checks
- [ ] **TESTS**: Verify with `BYRO_VALIDATION=sync` before/after
