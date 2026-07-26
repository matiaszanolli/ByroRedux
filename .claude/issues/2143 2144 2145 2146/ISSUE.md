## Issue 2143 [OPEN] CONC-D1-2026-07-25-01: Presentation render pass suppresses its implicit outgoing dependency with dstStageMask = NONE
labels: bug low vulkan 

## Severity
LOW

## Dimension
Vulkan Queue & AS Sync — `/audit-concurrency` 2026-07-25

## Location
`crates/renderer/src/vulkan/presentation.rs:173-178`

## Description
The presentation render pass declares an explicit `srcSubpass = 0 → dstSubpass = VK_SUBPASS_EXTERNAL` dependency with `dstStageMask = NONE` and no `dstAccessMask` — replacing Vulkan's implicit end-of-pass dependency with one whose second sync scope is empty, leaving the pass's `COLOR_ATTACHMENT_WRITE` unordered against any later command. Every sibling pass (composite, egui) declares a real dst scope; presentation is the only outlier.

## Evidence
`.dst_stage_mask(vk::PipelineStageFlags::NONE)` at `presentation.rs:173-178`, versus `composite.rs:547-555` (`COMPUTE_SHADER|TRANSFER`) and `egui_pass.rs:322-328` (`BOTTOM_OF_PIPE`, with an explicit "don't rely on the implicit edge" comment).

## Impact
No live hazard today — the two current downstream consumers (egui overlay, screenshot copy) each carry their own incoming barrier with a matching src scope, and the present itself is covered by the `render_finished` semaphore. The exposure is forward-looking: a future pass added between `presentation.dispatch` and `end_command_buffer` without its own barrier would race the swapchain image with nothing to catch it in `cargo test`.

## Trigger Conditions
Not reproducible today; requires a future code change adding an unbarriered swapchain-image consumer.

## Verification Path
`BYRO_VALIDATION=sync` on a screenshot-capture frame and an egui-overlay frame; absence of `SYNC-HAZARD-READ_AFTER_WRITE` on the swapchain image is the evidence the two self-synchronizing consumers are sufficient today.

## Related
`composite.rs:547-555` (the pattern to mirror), commit `33d6a18e`.

## Suggested Fix
Give the outgoing dependency a real dst scope mirroring the actual consumers (`COLOR_ATTACHMENT_OUTPUT | TRANSFER` / `COLOR_ATTACHMENT_READ|WRITE | TRANSFER_READ`), or delete the explicit dependency and let Vulkan synthesize the implicit one. Confirm with `BYRO_VALIDATION=sync` before/after.

## Completeness Checks
- [ ] **TESTS**: Verify with `BYRO_VALIDATION=sync` before/after

---
## Issue 2144 [OPEN] CONC-D1-2026-07-25-02: HYPOTHESIS — swapchain layout transition may not be covered by the acquire semaphore's wait stage
labels: bug low vulkan 

## Severity
LOW

## Dimension
Vulkan Queue & AS Sync — `/audit-concurrency` 2026-07-25

## Status note
HYPOTHESIS, not a confirmed bug. Strong prior this is a false positive (see Impact).

## Location
`crates/renderer/src/vulkan/presentation.rs:143-172` + `crates/renderer/src/vulkan/context/draw.rs:2249-2250`

## Description
The submit waits on `image_available[frame]` with `wait_dst_stage_mask = [COLOR_ATTACHMENT_OUTPUT]`; the presentation pass's swapchain attachment has `initial_layout = UNDEFINED` and an incoming dependency whose dst scope includes `FRAGMENT_SHADER`, which the acquire wait does not block. In principle the `UNDEFINED → COLOR_ATTACHMENT_OPTIMAL` transition (and the implicit discard) could execute before the presentation engine finished reading the image for its previous present.

## Evidence
`presentation.rs:143-144,166-169` (layout declarations); `draw.rs:2249-2250` (`wait_stages = [COLOR_ATTACHMENT_OUTPUT]`).

## Impact
If real — intermittent tearing/partial-frame corruption of the previous frame under MAILBOX or rapid resize, on drivers where the from-UNDEFINED transition isn't a no-op. Strong prior this is a false positive: the identical shape existed when `composite` owned the swapchain write, and sync-val was run against exactly this construct without flagging an acquire-ordering hazard (it did flag an unrelated WAW, already fixed).

## Trigger Conditions
Requires a driver that returns the acquired index optimistically and signals the semaphore later. Not reproducible on demand.

## Verification Path
`BYRO_VALIDATION=sync` on a release build, looking for `SYNC-HAZARD-WRITE_AFTER_PRESENT`/`WRITE-AFTER-READ` naming the swapchain image at the presentation render-pass begin. Absent that message, close as false positive, not "fixed."

## Related
`composite.rs:512-524` (comment recording the prior sync-val run), project rule against speculative Vulkan sync fixes.

## Suggested Fix
Do not change anything on this reasoning alone. Only if sync-val confirms, add `FRAGMENT_SHADER` to `wait_stages` at `draw.rs:2250` (cheap, no render-pass surgery).

## Completeness Checks
- [ ] **TESTS**: N/A pending validation-layer confirmation

---
## Issue 2145 [OPEN] CONC-D1-2026-07-25-03: FSR dispatch-failure recovery depends on undocumented FFX partial-recording behaviour
labels: bug low vulkan 

## Severity
LOW

## Dimension
Vulkan Queue & AS Sync — `/audit-concurrency` 2026-07-25

## Location
`crates/renderer/src/vulkan/frame_upscaler.rs:441-473`, `:479-521`

## Description
`FrameUpscaler::record` treats an `ffxDispatch` error as "nothing was recorded except my own boundary barriers," but the vendored SDK's `ExecuteGpuJobsVK` records every queued job into the command buffer and only checks the error code after the loop — a mid-sequence failure can already have recorded barriers/dispatches. The recovery path happens to be correct today (verified independently: FFX transitions land on states the pre-barriers already established as a no-op, and the blit's src stage/access mask includes `COMPUTE_SHADER`, ordering any partial FFX storage writes before the recovery blit's transfer write) — but the correctness is incidental, not designed.

## Evidence
`ffx_vk.cpp:4198-4236` records all jobs before checking `errorCode`; `frame_upscaler.rs:441-467` recovery assumes only its own barriers ran.

## Impact
A future narrowing of the "over-broad" blit masks (an attractive-looking cleanup) would silently reintroduce a same-command-buffer WAW with no test coverage.

## Trigger Conditions
Requires an actual `ffxDispatch` failure (SDK OOM, internal overflow, device-lost mid-frame). Not reproducible on demand; one-shot latch per swapchain generation.

## Verification Path
Not reachable by `cargo test`. Validation-layer confirmation needs a fault-injected dispatch failure; practical mitigation is documentation plus keeping the currently over-broad masks.

## Related
Same underlying SDK behaviour as CHAIN-D2-03 (filed separately). commit `f9a42e07`, `frame_upscaler.rs:808-818` (`blit_output_src_access`, unit-tested).

## Suggested Fix
Add a comment at `frame_upscaler.rs:441` recording that FFX `ExecuteGpuJobsVK` records all jobs before checking its error code, so the wide src mask on the recovery blit is documented as load-bearing, not defensive padding. No code change required.

## Completeness Checks
- [ ] **TESTS**: Documentation-only fix, no test required

---
## Issue 2146 [OPEN] CHAIN-D2-01: FSR error propagation in record_post_passes can bypass the #917 no-advance-on-unsubmitted-dispatch invariant
labels: bug low vulkan 

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

---
