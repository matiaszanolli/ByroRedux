# 2151: CHAIN-D2-04: Single shared depth image is now also layout-transitioned by the FSR pass late in the frame

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/2151
**Labels**: bug, low, vulkan

---

## Severity
LOW

## Dimension
Compute → AS → Fragment Chains — `/audit-concurrency` 2026-07-25

## Location
`crates/renderer/src/vulkan/context/mod.rs:1168` (`depth_image`, single not per-FIF); `crates/renderer/src/vulkan/frame_upscaler.rs:633-646`

## Description
`depth_image` is a single image shared by all frame-in-flight framebuffers, unlike every color attachment (explicitly per-FIF to remove cross-frame hazards). Historically the only late-frame readers were SSAO/SVGF (same-layout `SHADER_READ`); FSR now additionally performs two **layout transitions** on it per frame. With `MAX_FRAMES_IN_FLIGHT = 2`, the frame-entry fence wait is on `in_flight[frame]` (frame N-1), not frame N, so frame N+1's render pass could begin writing depth while frame N's FSR transition is still executing.

## Evidence
`draw.rs:735-738` documents the per-FIF color design explicitly; depth is the one attachment that doesn't follow it.

## Impact
A cross-frame WAW/WAR on depth would surface as flickering depth-dependent effects (SSAO shimmer, FSR disocclusion artefacts), not a crash — likely benign given in-order queue execution on current drivers, but unconfirmed.

## Trigger Conditions
Frame overlap — any frame where the GPU hasn't finished frame N by the time frame N+1's render pass starts. Normal at high frame rates.

## Verification Path
`BYRO_VALIDATION=1` with sync validation, FSR mode, 300+ frames of camera motion. Confirming signal: `SYNC-HAZARD-WRITE-AFTER-READ`/`-WRITE` naming the depth image at render-pass begin. A clean 300-frame run is meaningful evidence of non-issue.

## Related
#1583 (closed), commit `d822a783`.

## Suggested Fix
If validation fires, make depth per-FIF like every other attachment (`Vec<vk::Image>` indexed by frame). Do not add speculative barriers first.

## Completeness Checks
- [ ] **TESTS**: N/A pending validation-layer confirmation
