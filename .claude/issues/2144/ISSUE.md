# 2144: CONC-D1-2026-07-25-02: HYPOTHESIS — swapchain layout transition may not be covered by the acquire semaphore's wait stage

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/2144
**Labels**: bug, low, vulkan

---

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
