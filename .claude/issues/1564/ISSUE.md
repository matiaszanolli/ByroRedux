# bug, low, pipeline, tech-debt

## REN-D11-03: No automated test pins fragment-output count to render-pass color-attachment count (7-way match hand-maintained across 4 sites)

**Severity**: LOW
**Dimension**: Pipeline/RenderPass
**Source audit**: `docs/audits/AUDIT_RENDERER_2026-06-14.md`
**Status**: NEW

## Description
SPIR-V reflection validates descriptor-set layouts but explicitly excludes fragment color **outputs**; nothing asserts `attachment_count == 7` or `frag_output_count == attachment_count`. The "7" is hand-replicated across the render pass + 4 pipeline factories.

## Evidence
- `crates/renderer/src/vulkan/reflect.rs` doc (line ~62) excludes outputs ("Non-descriptor `OpVariable`s — inputs, outputs, push constants — ...").
- Each pipeline factory hardcodes a 7-element attachment array: `pipeline.rs` (blend arrays), `water.rs` (`build_pipeline`), `context/helpers.rs` (`create_render_pass`).
- No `#[test]` pins the count; no shared `MAIN_PASS_COLOR_ATTACHMENT_COUNT` const exists.

## Impact
A future G-buffer attachment add/remove must be mirrored across 5 sites by hand; a miss is `cargo test`-invisible (validation error / water OOB blend-state read at runtime). Tech-debt, not a current defect.

## Suggested Fix
Add a shared `MAIN_PASS_COLOR_ATTACHMENT_COUNT = 7` const + a unit test asserting each pipeline's blend-array length equals it (compile-time/array-length only — no barrier state, no RenderDoc).

## Completeness Checks
- [ ] **SIBLING**: every pipeline factory (opaque/blend/water/UI) references the shared const, not a literal `7`
- [ ] **TESTS**: a unit test pins each factory's blend-array length to the const
