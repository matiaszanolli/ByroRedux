# REN-2026-07-05-L03: shader-pipeline.md G-buffer row claims reverse-Z depth; the renderer uses standard depth

**Issue**: #1894 · **Severity**: LOW · **Labels**: low, renderer, documentation
**Dimension**: Denoiser/Composite (G-buffer) · **Filed from**: docs/audits/AUDIT_RENDERER_2026-07-05.md (rt-deep suite)
**Location**: docs/engine/shader-pipeline.md (G-Buffer Layout, Depth row) — stale;
crates/renderer/src/vulkan/pipeline.rs, context/draw.rs, crates/renderer/shaders/composite.frag — authoritative

## Description
The G-buffer table says `Depth | D32_SFLOAT | Reverse-Z depth (1.0 = camera near, 0.0 = far)`.
The renderer uses **standard** depth: near ≈ 0, far = 1.

## Evidence
- shader-pipeline.md:97 — the reverse-Z row.
- pipeline.rs:391,615 — depth_compare_op(LESS_OR_EQUAL) both pipelines; viewport min 0.0 max 1.0.
- draw.rs::draw_frame clears depth to 1.0 (= far, standard).
- composite.frag:106 "we use standard depth where 1.0 = far"; :323 `depth >= 0.9999` = sky.

## Impact
Latent trap: a depth-pipeline edit trusting the doc and switching to reverse-Z (clear 0.0,
GREATER) would silently invert composite sky detection + all `depth < 0.9999` fog/volumetric
branches. Invisible to cargo test.

## Suggested Fix
Correct the Depth row to "standard depth (0.0 = near, 1.0 = far), LESS_OR_EQUAL, clear = 1.0".
