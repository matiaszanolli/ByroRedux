# #953 — REN-D1-NEW-05: images_in_flight invariant is implicit

**Source**: `docs/audits/AUDIT_RENDERER_2026-05-11_DIM1.md`
**Dimension**: Vulkan Sync
**Severity**: LOW
**Confidence**: MED
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/953

## Locations

- `crates/renderer/src/vulkan/sync.rs:69-71` (field declaration)
- `crates/renderer/src/vulkan/context/draw.rs:179-187` (the guard that relies on the invariant)
- `crates/renderer/src/vulkan/context/draw.rs:144-156` (upstream both-slots wait that upholds it)

## Summary

The `images_in_flight` aliasing guard at draw.rs:180 is correct today only because the both-slots wait at draw.rs:144-156 guarantees both frame fences are signaled before any image-fence read. The invariant is split across two files and unstated. A future single-slot refactor would silently break it.

## Fix (preferred)

Docs-only: rustdoc the invariant on `FrameSync::images_in_flight`, citing the both-slots wait as the upstream guarantee.

Optional: add a `debug_assert!` on `vkGetFenceStatus(image_fence) == SUCCESS` at draw.rs:184 as a debug-only safety net.

## Tests

N/A for docs-only fix.
