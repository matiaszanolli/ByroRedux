# REN-D8-09: `fog_mean_density_scale` redoes a pure 8 192-iteration f64 loop every frame

Labels: low,renderer,performance,bug
Filed from: volumetrics-deep audit suite, 2026-11-09

**Severity**: LOW (hot-path CPU; **not measured**). **Status**: NEW.

**Location**: `fog_mean_density_scale` (`crates/renderer/src/vulkan/volumetrics/noise.rs`), called from `build_composite_params` (`crates/renderer/src/vulkan/context/frame_params.rs`) every frame, including frames where the beyond-grid tail is off.

## Description
The histograms are cached in a `OnceLock`, but the 256 x 32 mean is recomputed per call with `smoothstep` and clamps in f64. Its only input is the coverage, which changes on weather transitions.

## Impact
Estimated tens of microseconds per frame on the render thread (unmeasured — measure before claiming a number; the project treats render-thread CPU time as a defect).

## Suggested Fix
Memoise on the coverage's bit pattern (in the context, not a global), or skip the call when `height_fog_params.w` (tail enabled) is 0.

Report: `docs/audits/AUDIT_RENDERER_2026-11-09.md`.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix (assert the cached value equals a fresh evaluation)

