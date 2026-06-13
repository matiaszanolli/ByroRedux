## Finding REN2-12 — Renderer Audit 2026-06-11

- **Severity**: LOW
- **Dimension**: TAA
- **Location**: `crates/renderer/shaders/taa.comp:235-263` (alpha at `:256`, `pixelStatic` branch at `:249`); `crates/renderer/src/vulkan/taa.rs:684-688`
- **Status**: NEW — residual of `2f7bcf78` (the original suggested fix's "normal α=0.1 fall-through" was not shipped). Validated CONFIRMED at HEAD `1e8a25ab`.

## Description

The YCoCg clamp is correctly re-armed for moving pixels (`!pixelStatic`), but `alpha` stays the single global value from `params.params.x` — which `taa.rs:684-688` sets to `1.0/(static_frames+1)` while the camera is parked (down to 1/256 after ~4 s). There is no per-pixel `pixelStatic ? ... : max(..., 0.1)` floor; the `pixelStatic` branch only affects the clamp, never alpha. History can pin at the clamp boundary, causing soft detail-loss on moving actors during long parked-camera scenes. Far milder than the pre-fix #1479 artifact.

## Suggested Fix

`float alpha = pixelStatic ? params.params.x : max(params.params.x, 0.1);` + recompile `taa.comp.spv`.

## Completeness Checks
- [ ] **SIBLING**: Verify the SVGF temporal pass doesn't have the same parked-camera α coupling
- [ ] **TESTS**: Regression check via the bench + byro-dbg flow if a visual pin exists; otherwise shader-source assertion in the lockstep tests

---
Source: `docs/audits/AUDIT_RENDERER_2026-06-11.md` · Filed by `/audit-publish`
