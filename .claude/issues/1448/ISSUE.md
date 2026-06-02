# #1448 — SYNC-01: Screenshot readback uses stale extent after same-frame resize

_Snapshot as filed (2026-06-02) from AUDIT_RENDERER_2026-06-02.md. GitHub is authoritative for live state._

- **Severity**: LOW
- **Dimension**: Vulkan Sync
- **Location**: `crates/renderer/src/vulkan/context/screenshot.rs:16-72` (readback) vs `:79-173` (record)
- **Status**: NEW

## Description
`screenshot_record_copy` sizes staging + copy region at frame-N extent and sets `screenshot_pending_readback`; `screenshot_finish_readback` at frame N+1 re-reads `self.swapchain_state.extent` (possibly changed by a `recreate_swapchain` between the two). Neither `recreate_swapchain` nor the resize path clears the pending flag or invalidates staging.

## Evidence
`screenshot.rs:26-27` re-derives width/height from the live extent; `:52` `write_image` uses them over old-extent staging data. The slice read is bounds-checked (`&slice[..size]`) — **not** memory-unsafe.

## Impact
A screenshot requested on a resize frame produces a corrupt / mis-dimensioned PNG. `byro-dbg` tooling only; not the render hot path.

## Suggested Fix
Capture `(width,height)` into the `screenshot_staging` tuple at record time and read those back in `screenshot_finish_readback` instead of re-deriving from the live extent; or clear `screenshot_pending_readback` in `recreate_swapchain`.

## Completeness Checks
- [ ] **TESTS**: regression covering a screenshot requested on the same frame as a resize
- [ ] **SIBLING**: confirm no other deferred-readback path (e.g. timer resolve) re-reads live extent

_Filed from [docs/audits/AUDIT_RENDERER_2026-06-02.md](../blob/main/docs/audits/AUDIT_RENDERER_2026-06-02.md)._
