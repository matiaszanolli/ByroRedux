# #961 — REN-D10-NEW-04: SVGF UBO host barrier per-dispatch — fold into bulk

**Source**: `docs/audits/AUDIT_RENDERER_2026-05-11_DIM10.md`
**Dimension**: Denoiser & Composite
**Severity**: LOW
**Confidence**: HIGH
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/961

## Location

`crates/renderer/src/vulkan/svgf.rs:828-841` — separate HOST→COMPUTE memory barrier per dispatch.

## Summary

Mirror of closed #909 (composite UBO host barrier folded into bulk). SVGF still emits its own per-dispatch barrier; the bulk pre-render barrier in `draw.rs` should be widened to also cover SVGF (and any other per-pass UBOs like TAA).

## Fix (preferred)

Hoist HOST→COMPUTE into the bulk barrier; widen `dst_stage_mask` to include COMPUTE_SHADER alongside FRAGMENT.

## Tests

N/A — barrier emissions aren't unit-tested. RenderDoc / validation layer verification.
