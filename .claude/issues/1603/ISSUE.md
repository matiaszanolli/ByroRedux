# CONC-2026-06-14-04: Screenshot straggler survives cancel() — renderer screenshot_pending_readback latch not cleared

- **Issue**: #1603
- **Severity**: LOW
- **Labels**: low, sync, renderer, bug
- **Dimension**: Worker Threads (Streaming, Debug)
- **Location**: `crates/renderer/src/vulkan/context/screenshot.rs:16-71` (`screenshot_finish_readback`) + `crates/core/src/ecs/resources.rs:140-155` (`ScreenshotBridge::cancel`) + `crates/debug-server/src/system.rs:72-78` (drain cancel path)
- **Source**: `docs/audits/AUDIT_CONCURRENCY_2026-06-14.md` (CONC-2026-06-14-04)
- **Status when filed**: NEW, CONFIRMED — residual of #1011/#1007.

## Description
`cancel()` clears `requested`/`result`/`owner` but has no handle to the renderer-private `screenshot_pending_readback` latch. If the copy was already recorded (`pending_readback = Some`) but the readback hasn't completed, `cancel()` clears `result` *before* the readback writes it; the straggler reappears post-cancel with `owner == NONE` and can be served to the next claimant's `take_result_for`.

## Trigger Conditions
Screenshot copy recorded → engine stalls > 5 s → client `recv_timeout` fires → drain calls `cancel()` → engine resumes, latched readback writes stale PNG with `owner == NONE` → next screenshot request reads the stale bytes first.

## Impact
Debug-server screenshot client can receive the previous (cancelled) frame's pixels. Debug/diagnostic only; loopback-only (#857); no crash, no unsafety, no World corruption.

## Suggested Fix
Tag each capture with a `u64` generation/capture-id and reject stale ids in `take_result_for`; or a "discard next readback" atomic checked by `screenshot_finish_readback`; lowest-touch: skip the `screenshot_result` write when no live claim exists. Mirror #1006 owner-tagging discipline.

## Related
#1011 / #1007 (cancel straggler fix this is a residual of); #1448 (stale extent, CLOSED); #1174 (poison panic, CLOSED); #1006 (owner-tagging).
