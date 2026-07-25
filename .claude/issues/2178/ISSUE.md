# 2178: PERF-D3-03: FrameUpscaler::create_outputs leaks its gpu-allocator sub-allocation if bind_image_memory fails

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/2178
**Labels**: bug, low, performance

---

## Severity
LOW

## Dimension
GPU Memory Pressure (Dim 3) — `/audit-performance` 2026-07-25

## Location
`crates/renderer/src/vulkan/frame_upscaler.rs` (`create_outputs`)

## Description
`FrameUpscaler::create_outputs` doesn't free its `gpu-allocator` sub-allocation if `bind_image_memory` fails, unlike the established pattern elsewhere in the renderer (e.g. `exposure.rs`) that frees the allocation on a subsequent bind failure.

## Impact
Unreachable except on driver OOM / genuine allocation failure — a narrow error path, but a real leak if it fires (the sub-allocation is never returned to the allocator's free list).

## Related
Same shape exists in `gbuffer.rs` per the audit's own note — fix both together.

## Suggested Fix
Free the `gpu-allocator` sub-allocation on the `bind_image_memory` failure branch in `FrameUpscaler::create_outputs`, mirroring `exposure.rs`'s pattern. Apply the same fix to the equivalent site in `gbuffer.rs`.

## Completeness Checks
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **SIBLING**: Same pattern checked and fixed in `gbuffer.rs`
