# 2146: CHAIN-D2-01: FSR error propagation in record_post_passes can bypass the #917 no-advance-on-unsubmitted-dispatch invariant

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/2146
**Labels**: bug, low, vulkan

---

## Severity
LOW

## Dimension
Compute → AS → Fragment Chains — `/audit-concurrency` 2026-07-25

## Location
`crates/renderer/src/vulkan/context/post_passes.rs:568-590`; `frame_upscaler.rs:358-359`; `crates/renderer/src/vulkan/svgf.rs:1287`; `crates/renderer/src/vulkan/taa.rs:770`

## Description
FSR introduced the first `?`-propagating error path inside `record_post_passes`. It sits after `svgf.dispatch`/`taa.dispatch` have already set `dispatched_this_frame`, and aborts `draw_frame` before `queue_submit`, so `mark_frame_completed()` never runs for that frame for SVGF/TAA — the latch stays `true` and a later frame's `mark_frame_completed` bumps `frames_since_creation[frame]` for a dispatch that never reached the GPU. This is precisely the failure mode #917 was written to prevent.

## Evidence
`frame_upscaler.rs:358` is the sole `Err` return in `FrameUpscaler::record`; `svgf.rs:1287` sets `dispatched_this_frame[frame] = true` with a comment that no longer covers errors introduced *after* that point.

## Impact
`frames_since_creation[frame]` over-advances by one, so `should_force_history_reset` can close one frame early — a one-frame smear/ghost on SVGF and TAA history.

## Trigger Conditions
Requires `FrameUpscaler::record` to observe `is_fsr_dispatch_active() == true` while `fsr_frame` is `None`. Both are derived from the same predicate within one `draw_frame` today, so this path is **unreachable as written** — it becomes reachable the moment a second `Err` return is added to `record`, or the jitter gate and record gate stop reading the same predicate.

## Verification Path
Not a Vulkan-sync claim — a host-side state-machine claim, verifiable by a unit test that calls `svgf.dispatch` then skips `mark_frame_completed` and asserts `frames_since_creation` did not advance on the next frame. No RenderDoc needed.

## Related
#917 (closed, the invariant this could bypass), #1932 (closed, TAA-D13-01, same class), #479 (closed).

## Suggested Fix
Either make `record_post_passes` infallible by latching the upscaler failure the same way SVGF/TAA/caustic do (`log::error!` + `dispatch_failure`, return `Ok`), or clear `svgf.dispatched_this_frame`/`taa.dispatched_this_frame` on the `draw_frame` error-return path alongside `recreate_image_available_for_frame`. The former is the smaller change and matches convention.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix (unit test per Verification Path above)
