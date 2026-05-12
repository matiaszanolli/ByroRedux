# #962 — REN-D10-NEW-05: SVGF pre-dispatch image barrier over-specifies FRAGMENT_SHADER

**Source**: `docs/audits/AUDIT_RENDERER_2026-05-11_DIM10.md`
**Dimension**: Denoiser & Composite
**Severity**: LOW
**Confidence**: MED
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/962

## Location

`crates/renderer/src/vulkan/svgf.rs:866-875` — `src_stage_mask: COMPUTE | FRAGMENT` on pre-dispatch image barrier.

## Summary

The FRAGMENT_SHADER source is redundant: composite reads SVGF output through a different view (`indirectTex`), and prior-frame FRAGMENT consumption is serialized by the both-slots `wait_for_fences` at `draw.rs:144-156`. The only true previous-stage producer of the OUT slot is COMPUTE.

## Fix (preferred)

Drop `FRAGMENT_SHADER` from `src_stage_mask`; keep the post-dispatch FRAGMENT|COMPUTE dst widening (#653).

## Tests

N/A — RenderDoc + validation layer verification.
