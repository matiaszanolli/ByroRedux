# REN-D23-2026-08-07-03: A mid-frame dispatch failure presents a jittered-but-unresolved frame

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2519
**Finding ID**: REN-D23-2026-08-07-03 (source: `docs/audits/AUDIT_RENDERER_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 23 — FSR Upscaler
**Location**: `crates/renderer/src/vulkan/frame_upscaler.rs::FrameUpscaler::record` (dispatch-`Err` recovery arm)
**Status**: NEW

## Description
The projection jitter for frame N is chosen at the *top* of `draw_frame` from `is_fsr_dispatch_active()`, but `dispatch_failure` can be set later in the same frame inside `record`. On that one frame the geometry pass has already rendered with a sub-pixel FSR jitter offset applied, and the recovery path blits that jittered image straight through. No pass resolves it. This is the same class of hazard `taa_jitter`'s `!taa_failed` gate (#1932 / TAA-D13-01) was added to close on the TAA side; the FSR side has no equivalent for the failing frame itself (subsequent frames are correctly unjittered).

## Evidence
`draw.rs:1573` reads `is_fsr_dispatch_active()`; `frame_upscaler.rs` sets `self.dispatch_failure = Some(error.to_string())` inside the `if let Err(error) = dispatch` arm, then calls `record_native_blit` on the already-jittered `inputs.scene_color`.

## Impact
One frame of un-resolved sub-pixel offset, i.e. a single-frame image shift/shimmer. Reachable only via a genuine SDK error or `BYRO_FSR_FORCE_DISPATCH_FAIL=1`, and only once per swapchain generation (the latch suppresses further attempts).

## Related
#2140, #2146; `BYRO_FSR_FORCE_DISPATCH_FAIL` fault injection.

## Suggested Fix
Document it as accepted (one frame, degraded path) rather than adding machinery — or, if it matters, have the recovery arm also call `signal_temporal_discontinuity(1)` so nothing downstream reprojects against that frame.

## Completeness Checks
- [ ] **TESTS**: `BYRO_FSR_FORCE_DISPATCH_FAIL=1` fault-injection run confirms the one-frame artifact and, if fixed, its absence
