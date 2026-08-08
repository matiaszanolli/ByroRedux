# REN-D4-2026-08-07-03: record_upscale_pass consumes the shared depth image, extending the MAX_FRAMES_IN_FLIGHT==2 contract to an unenumerated consumer

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2485
**Finding ID**: REN-D4-2026-08-07-03 (source: `docs/audits/AUDIT_RENDERER_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 4 — Sync/Barriers
**Location**: `crates/renderer/src/vulkan/context/post_passes.rs::VulkanContext::record_upscale_pass` (`depth: self.depth_image`), against `crates/renderer/src/vulkan/sync.rs` (the `const _: () = assert!(MAX_FRAMES_IN_FLIGHT == 2, ...)` doc block)
**Status**: NEW (documentation/contract-completeness finding; the underlying hazard is #870, already mitigated)

## Description
`sync.rs`'s `MAX_FRAMES_IN_FLIGHT` const-assert doc enumerates the shared-depth-image consumers as "frame N's compute consumers (SSAO sampler, SVGF depth read)". FSR (`frame_upscaler.rs`, via `record_upscale_pass`) is a third consumer of the same single `self.depth_image` and is not named. The safety argument itself is unchanged and still correct — the both-slots fence wait covers all of them at `MAX_FRAMES_IN_FLIGHT == 2` — so this is not a live hazard.

## Evidence
`depth_image` is declared once (not a per-frame `Vec`, unlike `gbuffer.rs`'s per-FIF images); `record_upscale_pass` passes `depth: self.depth_image` into `UpscaleDispatchInputs` with no frame index.

## Impact
None today. The risk is that whoever next evaluates making the depth image per-frame-in-flight sizes the work off an incomplete consumer list and misses the FSR binding, or that the enumerated list is read as exhaustive during a future `MAX_FRAMES_IN_FLIGHT` bump review.

## Related
#870 / REN-D4-NEW-01 (the original shared-depth finding); #282 (the both-slots wait that makes it safe).

## Suggested Fix
Add the FSR/`frame_upscaler` depth read to the consumer list in `sync.rs`'s `MAX_FRAMES_IN_FLIGHT` doc block. Documentation-only; no barrier or pipeline change.

## Completeness Checks
- [ ] **TESTS**: N/A (doc-only change)
