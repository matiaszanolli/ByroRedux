# REN-2026-07-05-L02: shader-pipeline.md submission order + compute table omit the Session-49 à-trous SVGF pass

**Issue**: #1893 · **Severity**: LOW · **Labels**: low, renderer, documentation
**Dimension**: Denoiser/Composite · **Filed from**: docs/audits/AUDIT_RENDERER_2026-07-05.md (rt-deep suite)
**Location**: docs/engine/shader-pipeline.md (Per-Frame Submission Order + Compute table) — stale;
crates/renderer/src/vulkan/svgf.rs (`SvgfPipeline::dispatch`) — authoritative

## Description
The doc lists only `7 svgf_temporal.comp` then jumps to `14 [Composite render pass]`, and the
Compute table lists only `svgf_temporal.comp`. The live pipeline runs a 5-iteration à-trous
spatial chain (`svgf_atrous.comp`) immediately after the temporal dispatch in the same command
buffer; composite samples the à-trous final ping-pong slot.

## Evidence
- shader-pipeline.md:35 (compute table) + :65/:73 (submission order) — only svgf_temporal.
- svgf.rs — ATROUS_ITERATIONS=5, svgf_atrous.comp.spv dispatched after temporal; indirect_view(frame)
  returns the à-trous final slot. crates/renderer/shaders/svgf_atrous.comp exists + compiled.

## Suggested Fix
Add svgf_atrous.comp to the Compute table and insert the à-trous chain (with ping-pong barriers)
into the submission-order block after step 7.

**Related**: #1814 (Session-49 overhaul); #1895 (L04, same drift in svgf.rs docstring).
