# Issue #1634 — REG-02: screenshot extent capture (#1448) has no guard test

_Snapshot as filed (immutable). GitHub is authoritative for current state._

**Source:** `docs/audits/AUDIT_REGRESSION_2026-06-14.md` — REG-02 (PARTIAL hardening gap, LOW)

The fix for **#1448** is **present and correct**; this issue tracks the missing guard test, not a regression.

## Description
Screenshot extent is now captured at command-record time (`vk::Extent2D` stored in `screenshot_pending_readback`) and the readback reads the captured value, so it survives a same-frame swapchain resize. There is no automated test pinning this — the readback path is `byro-dbg`-driven, and the original issue marked TESTS as a gap.

## Evidence
- `crates/renderer/src/vulkan/context/screenshot.rs:171` — `self.screenshot_pending_readback = Some(vk::Extent2D { width, height });` (captured at record time)
- `crates/renderer/src/vulkan/context/screenshot.rs:17-26` — readback `.take()`s the captured extent and reads `extent.width` / `extent.height`, **not** live `swapchain_state.extent`.

## Impact
A revert to live-extent readback would silently re-arm SYNC-01 (readback against a resized swapchain → wrong dimensions / OOB copy). Invisible to `cargo test` today.

## Suggested Fix
Add a unit test asserting the readback path snapshots the recording-time extent (i.e. reads from the captured `screenshot_pending_readback` value rather than `swapchain_state.extent`), or accept as RenderDoc/manual-gated and leave a tracking comment naming the invariant.

## Completeness Checks
- [ ] **SIBLING**: Same capture-at-record-time pattern checked for any other deferred-readback path
- [ ] **TESTS**: A regression test pins that the readback uses the captured extent, not live swapchain extent
