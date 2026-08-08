# REN-D4-2026-08-07-01: Swapchain image's UNDEFINED to COLOR_ATTACHMENT_OPTIMAL layout transition is not provably ordered after the acquire semaphore

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2465
**Finding ID**: REN-D4-2026-08-07-01 (source: `docs/audits/AUDIT_RENDERER_2026-08-07.md`)

**Severity**: MEDIUM
**Dimension**: 4 — Sync/Barriers
**Location**: `crates/renderer/src/vulkan/presentation.rs::PresentationPipeline::recreate`/`create_render_pass` (the `incoming` `vk::SubpassDependency`), against `crates/renderer/src/vulkan/context/draw.rs::draw_frame` (`let wait_stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];`)
**Status**: NEW (adjacent to the invariant note in `docs/audits/AUDIT_RENDERER_2026-04-25.md`, which recorded `(initial_layout=UNDEFINED, image_available wait at COLOR_ATTACHMENT_OUTPUT)` as "the load-bearing invariant" back when *composite* was the swapchain writer; the writer has since moved to `presentation.rs` with a wider dependency and the invariant was not re-checked)

## Description
The submit waits on `image_available[frame]` with `wait_dst_stage_mask = COLOR_ATTACHMENT_OUTPUT` only. The presentation render pass is the pass that writes the acquired swapchain image; its color attachment is declared `initial_layout(UNDEFINED)` → `final_layout(PRESENT_SRC_KHR)`, so the pass performs an `UNDEFINED → COLOR_ATTACHMENT_OPTIMAL` layout transition. Per spec, a render pass's automatic layout transition is ordered between the first and second synchronization scopes of the relevant `SUBPASS_EXTERNAL` dependency. That dependency's `dst_stage_mask` is `FRAGMENT_SHADER | COLOR_ATTACHMENT_OUTPUT` — and `FRAGMENT_SHADER` is *logically earlier* than `COLOR_ATTACHMENT_OUTPUT`, so it is **not** gated by a semaphore wait scoped to `COLOR_ATTACHMENT_OUTPUT`. The transition (a write, for hazard purposes) therefore has a window in which it can execute before the presentation engine has released the image.

## Evidence
- `draw.rs`: `let wait_stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];`
- `presentation.rs`, attachment: `.load_op(vk::AttachmentLoadOp::DONT_CARE).initial_layout(vk::ImageLayout::UNDEFINED).final_layout(vk::ImageLayout::PRESENT_SRC_KHR)`
- `presentation.rs`, `incoming`: `.dst_stage_mask(vk::PipelineStageFlags::FRAGMENT_SHADER | vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)` — the `FRAGMENT_SHADER` limb exists to cover the upscaled-image sampler read (a different resource), but subpass-dependency scopes are pass-wide, so the swapchain attachment's transition inherits the looser ordering.

## Impact
Theoretical corruption / tearing of the presented image, and a potential WSI VUID hit under sync-validation. In practice `UNDEFINED` discards contents so a premature transition is mostly benign on current drivers, which is exactly why this is invisible to `cargo test` and to normal play. Blast radius is every frame, on every platform, if a driver ever schedules the transition early.

## Related
`AUDIT_RENDERER_2026-04-25.md` (the original invariant note); `AUDIT_RENDERER_2026-04-22.md` (same class of "pass-wide dependency masks a per-resource need" observation on composite); #2143 (which repaired the *outgoing* half of this same dependency pair).

## Suggested Fix
**Needs RenderDoc / sync-validation verification.** Run with `BYRO_VALIDATION=1` plus synchronization validation enabled and confirm whether the layer reports an acquire-ordering hazard on the swapchain image before changing anything. Do not blind-fix.

## Completeness Checks
- [ ] **TESTS**: Needs `BYRO_VALIDATION=1` sync-validation capture before any change is made
