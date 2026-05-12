# #963 — REN-D10-NEW-06: composite render-pass external dep lacks UNIFORM_READ

**Source**: `docs/audits/AUDIT_RENDERER_2026-05-11_DIM10.md`
**Dimension**: Denoiser & Composite
**Severity**: LOW
**Confidence**: MED
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/963

## Location

`crates/renderer/src/vulkan/composite.rs:404-415` — `dst_access_mask = SHADER_READ` only.

## Summary

The UBO host-write → fragment-uniform-read dependency is currently covered by #909's bulk pre-render barrier. If that barrier is restructured to omit composite, the render-pass external dep won't pick up the UBO read on its own. Defence-in-depth gap.

## Fix (preferred)

Add `vk::AccessFlags::UNIFORM_READ` to `composite_dep_in.dst_access_mask` so the dep stands on its own.

## Tests

N/A.
