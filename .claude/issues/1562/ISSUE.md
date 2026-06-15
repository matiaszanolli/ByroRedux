# documentation, low, pipeline

## REN-D11-01: shader-pipeline.md G-buffer table omits the 7th color attachment (ReSTIR-DI reservoir)

**Severity**: LOW
**Dimension**: Pipeline/RenderPass
**Source audit**: `docs/audits/AUDIT_RENDERER_2026-06-14.md`
**Status**: NEW

## Description
`docs/engine/shader-pipeline.md` says "Six colour attachments + depth"; the live main render pass declares **seven** color attachments — attachment 6 is the ReSTIR-DI reservoir (`RESERVOIR_FORMAT = R32G32B32A32_UINT`), with depth at attachment 7. The audit skill's checklist inherited the stale "six color attachments" figure from this doc.

## Evidence
- `crates/renderer/src/vulkan/context/helpers.rs` (`create_render_pass`): `color_refs = [0..=6]` (7), `depth_ref { attachment: 7 }`, logs "Render pass created (7 color + depth)".
- `crates/renderer/shaders/triangle.frag`: `layout(location = 6) out uvec4 outReservoir;`
- All opaque/blend/water/UI pipelines declare 7 color-blend entries (reservoir slot `blend_enable(false)`).
- `docs/engine/shader-pipeline.md:85` — "Six colour attachments + depth".

## Impact
Doc-only; code internally consistent. A contributor under-sizing a new pipeline's blend-attachment array to 6 would trip VUID-vkCmdDrawIndexed-blendEnable-04727 (OOB read sees `blendEnable=TRUE` on the integer reservoir attachment).

## Suggested Fix
Add the reservoir row + change "Six" → "Seven" in `shader-pipeline.md`.

## Completeness Checks
- [ ] **SIBLING**: the descriptor-set / G-buffer tables elsewhere in the same doc agree on 7 attachments
- [ ] **TESTS**: cross-reference with REN-D11-03 (a count const + test would make this drift `cargo test`-visible)
