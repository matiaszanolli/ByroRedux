# REN-D4-05: image-health readback buffer is allocated with the upload-only MemoryLocation::CpuToGpu instead of GpuToCpu

- **Severity**: MEDIUM
- **Dimension**: 4 — Sync/Barriers (host-access classification; borders Memory/Lifecycle)
- **Location**: `crates/renderer/src/vulkan/context/mod.rs` (the `image_health_buffers` construction loop in `VulkanContext::new`, via `GpuBuffer::create_host_visible`); `crates/renderer/src/vulkan/buffer.rs` (`create_host_visible` hard-codes `MemoryLocation::CpuToGpu`). Contrast: `crates/renderer/src/vulkan/context/screenshot.rs` — `ensure_screenshot_staging` (`GpuToCpu`).

## Description
The counter buffers are created through `GpuBuffer::create_host_visible`, whose allocation site pins `location: MemoryLocation::CpuToGpu` with no parameter to vary it. That location exists for staging *uploads*; gpu-allocator resolves it toward `HOST_VISIBLE | HOST_COHERENT` and on a discrete card frequently device-local BAR memory, i.e. uncached write-combined from the CPU's point of view. `GpuToCpu` is the readback location and additionally prefers `HOST_CACHED`. The image-health buffer is read by the host **every frame, on the hot path, right after the fence wait** — it is a readback buffer wearing an upload buffer's allocation.

## Evidence
`create_host_visible`'s `AllocationCreateDesc` hard-codes `location: MemoryLocation::CpuToGpu` with `name: "host_visible_buffer"`. `grep -rn "MemoryLocation::GpuToCpu" crates/renderer/src` returns exactly one site — the screenshot staging buffer. `IMAGE_HEALTH_BUFFER_BYTES = 8`, far below a typical `nonCoherentAtomSize` of 64, so any future flush/invalidate has to be atom-aligned and the aligned range can reach into neighbouring suballocations — a problem `GpuBuffer::flush_range` already grapples with on the write side.

Spot-checked against live code during publish: `buffer.rs:652` confirmed `location: MemoryLocation::CpuToGpu`; `context/mod.rs:2773-2794` confirmed `image_health_buffers` built via `create_host_visible`; `screenshot.rs:244` confirmed `MemoryLocation::GpuToCpu` is the only other site.

## Impact
Mis-tiered allocation on a per-frame host-read path, plus a coupling that makes the correct-looking fix dangerous in isolation: `GpuToCpu` steering toward `HOST_CACHED` is precisely the case where the missing-invalidate finding (REN-D4-04) becomes observable. No visual defect.

## Related
REN-D4-04 (the availability half — **must move together**, not separately filed in this pass), REN-D5-05 (the documentation half), #2736.

## Suggested Fix
Observation only for now. If acted on, the two findings must move together — switching to `GpuToCpu` first requires REN-D4-04's invalidate/availability step to exist, or the change trades a theoretical staleness risk for a likelier one.

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2752
