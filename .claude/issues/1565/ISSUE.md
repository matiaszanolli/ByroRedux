# documentation, renderer, low

## REN-D3-DOC-01: shader-pipeline.md + memory-budget.md say GpuCamera is 320 B; code is 336 B

**Severity**: LOW
**Dimension**: GPU-Struct Layout
**Source audit**: `docs/audits/AUDIT_RENDERER_2026-06-14.md`
**Status**: NEW

## Description
`GpuCamera` grew 320→336 B with the `render_origin: [f32;4]` field (#markarth-precision / #1492). Code is consistent (`gpu_camera_is_336_bytes` pins 336); two docs still say 320.

## Evidence
- `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs` — `GpuCamera` with `render_origin: [f32; 4]`; size pinned by `gpu_camera_is_336_bytes`.
- `docs/engine/shader-pipeline.md:105` "320 bytes" + table line 247 "(320 B)".
- `docs/engine/memory-budget.md:23` "Camera UBO ... 320 B".

## Impact
Doc-only. A reader sizing a UBO from the doc under-allocates by 16 B.

## Suggested Fix
Update both docs to 336 B and add the `render_origin` row to the GpuCamera field table.

## Completeness Checks
- [ ] **SIBLING**: both doc sites (shader-pipeline.md + memory-budget.md) and the descriptor-set table are updated together
