# #952 — REN-D1-NEW-04: reset_fences before fallible recording

**Source**: `docs/audits/AUDIT_RENDERER_2026-05-11_DIM1.md`
**Dimension**: Vulkan Sync
**Severity**: LOW
**Confidence**: HIGH
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/952

## Locations

- `crates/renderer/src/vulkan/context/draw.rs:189-193` (reset)
- `crates/renderer/src/vulkan/context/draw.rs:2090` (submit, re-signals fence)
- `crates/renderer/src/vulkan/context/draw.rs:144-156` (both-slots wait that would deadlock)
- `crates/renderer/src/vulkan/sync.rs:140-155` (doc comment that describes the same failure mode for the resize path)

## Summary

`reset_fences` at draw.rs:191 runs before ~1900 lines of fallible operations and the `queue_submit` at 2090 that re-signals the fence. Any `?`-propagated mid-frame error leaves the fence UNSIGNALED with no pending submit, deadlocking the next frame's both-slots wait. Hot-path mirror of the resize-path issue closed by #908 / REN-D1-NEW-01.

## Fix (preferred)

Move `reset_fences` to immediately before `queue_submit`. Canonical Khronos pattern; single-line move.

## Tests

Regression test that injects a mid-frame error and asserts the next `draw_frame` does not deadlock. May require a test-only error-injection hook.
