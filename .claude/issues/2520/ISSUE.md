# REN-D23-2026-08-07-04: UpscalerMode::Taa pays a full-resolution 1:1 image blit every frame that produces a byte-identical image

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2520
**Finding ID**: REN-D23-2026-08-07-04 (source: `docs/audits/AUDIT_RENDERER_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 23 — FSR Upscaler
**Location**: `crates/renderer/src/vulkan/frame_upscaler.rs::FrameUpscaler::record_native_blit`
**Status**: NEW

## Description
In `UpscalerMode::Taa`, `FrameExtentSet::for_output` sets `render == output`, so the bridge's `cmd_blit_image` src/dst offsets are identical and the `LINEAR` filter degenerates to an exact copy. Every TAA-mode frame therefore reads and writes a full-resolution `R16G16B16A16_SFLOAT` image (~16 MB of traffic at 1080p, ~66 MB at 4K) plus two pipeline barriers, purely to move data into a target `presentation.frag` could have sampled directly. The module doc frames the split as deliberate ("keeps scene composition and presentation decoupled, and gives FSR one explicit frame-graph slot"), which is a sound design argument — the cost is the part that isn't documented.

## Evidence
`upscaling.rs::FrameExtentSet::for_output` — `UpscalerMode::Taa => output`; `record_native_blit` builds `src_offsets` from `self.extents.render` and `dst_offsets` from `self.extents.output`.

## Impact
Pure bandwidth on the non-default path. Not a correctness issue. Grows with output resolution.

## Related
`docs/engine/fsr3-upscaler-integration-plan.md` phase 4 (native bridge).

## Suggested Fix
If TAA mode ever matters for perf again, let `PresentationPipeline` bind composite's scene view directly when `render == output` and skip the blit; otherwise add the cost to the module doc so the next reader does not rediscover it. Do NOT re-bench the FSR matrix off the back of this — it does not touch the FSR path.

## Completeness Checks
- [ ] **TESTS**: N/A unless the blit-skip optimization is implemented, in which case a bench confirms the bandwidth saving
