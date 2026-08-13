# REN-D4-04: device→host readbacks make no availability/invalidate step

**Severity**: HIGH (Vulkan spec violation)
**Dimension**: 4 — Sync/Barriers
**Location**: `crates/renderer/src/vulkan/context/resources.rs` — `collect_image_health`; `crates/renderer/src/vulkan/context/draw.rs` — the `collect_image_health(frame)` call site; `crates/renderer/src/vulkan/context/screenshot.rs` — `screenshot_finish_readback`. Contrast: `crates/renderer/src/vulkan/buffer.rs` — `GpuBuffer::is_coherent` / `flush_mapped` / `flush_range`.

## Description

`#2736` added a per-frame device→host readback justified by "the fence proves submission completed, so the host read needs no barrier." The premise is right, the conclusion is not: a fence's memory dependency covers *device* access only, per spec. `GpuBuffer` models the non-coherent case for host→device writes but has no `invalidate_mapped` counterpart for device→host reads.

## Evidence

`grep -rn "HOST_READ\|invalidate_mapped" crates/renderer/src` → zero hits. All six `PipelineStageFlags::HOST` uses are `HOST_WRITE` sources, none a destination stage.

## Impact

On a non-coherent/cached host-visible memory type, the host read may return stale cache lines — feeds `ImageHealth`, the bench summary, and the exterior smoke gate's hard-fail. Currently benign on the dev card (gpu-allocator picks a coherent type), which is why this needs a device to falsify.

## Suggested Fix

**Do not blind-fix** — needs a `BYRO_VALIDATION=1` run first. The documentation half (the three comments asserting fence-sufficiency) is safe to correct independently.

## Related

#2736, #2484

Filed from `docs/audits/AUDIT_RENDERER_2026-08-12b.md` (finding REN-D4-04).
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2740
