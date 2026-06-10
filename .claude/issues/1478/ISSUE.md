# #1478 — REN-D23-NEW-01: GPU timer vkResetQueryPool without hostQueryReset on RT-absent devices

_Snapshot as filed 2026-06-09 from docs/audits/AUDIT_RENDERER_2026-06-09.md. GitHub is authoritative for live state._

**Severity**: HIGH (Vulkan spec violation → ≥ HIGH per severity rules)
**Dimension**: Debug Overlay & GPU Telemetry
**Source**: `docs/audits/AUDIT_RENDERER_2026-06-09.md`
**Status**: NEW (distinct from open egui issues #1433/#1427)

## Description
`GpuPerFrameTimers::new()` gates creation purely on `caps.timestamp_supported` (`crates/renderer/src/vulkan/gpu_timers.rs:188`). Both `new()` (`:211`) and per-frame `read_and_reset()` (`:298`) call the **host-side** `device.reset_query_pool(...)`, which requires the `hostQueryReset` feature (VK_KHR_host_query_reset). But that feature is enabled only when ray queries are present: `.host_query_reset(caps.ray_query_supported)` (`crates/renderer/src/vulkan/device.rs:475`).

RT is optional in device selection: `is_device_suitable` requires only the swapchain extension + `synchronization2`; `ray_query_supported` is an optional probe, never a rejection criterion. `timestamp_supported` (`timestampComputeAndGraphics == TRUE`) is independent of RT and is reported by many non-RT GPUs.

## Evidence
- `device.rs:475` — `.host_query_reset(caps.ray_query_supported)` (feature gated on RT).
- `gpu_timers.rs:188` — timer creation gated on `timestamp_supported` alone.
- On a timestamp-capable, RT-less device: timers are created and immediately call `reset_query_pool` with the feature disabled → VUID-vkResetQueryPool-None-02665, at init and once per frame thereafter.
- The module doc (`gpu_timers.rs:64-70`) asserts the reverse ("timestamp support implies the RT gate") — that is backwards; and line 5 already *claims* `cmd_reset_query_pool` is used, so code and comment disagree.

## Impact
Per-frame Vulkan spec violation / UB on any timestamp-capable GPU lacking the RT extension set. Does **not** affect the project's stated RT-mandatory target hardware (RTX 4070 Ti — `ray_query_supported` always true there) in normal operation, but it is reachable because device selection does not require RT.

## Suggested Fix
Any of:
1. Track the enabled `hostQueryReset` in `DeviceCapabilities` and gate `GpuPerFrameTimers::new()` on `timestamp_supported && host_query_reset_enabled`; **or**
2. Switch the resets to a command-buffer `cmd_reset_query_pool` at the top of `draw_frame` (no host feature required — matches the line-5 doc claim); **or**
3. Enable `host_query_reset` unconditionally (widely supported, cheap).
If the engine is truly RT-mandatory, also consider rejecting non-RT devices in `is_device_suitable`.

## Related
#1194 (GPU telemetry instrumentation).

## Completeness Checks
- [ ] **SIBLING**: check every other host-side feature gated on `ray_query_supported` for the same "used without the gate" hazard (e.g. BLAS-compaction-only features).
- [ ] **DROP**: if the fix moves to `cmd_reset_query_pool`, verify query-pool reset still happens before each re-record (and the `:298` host reset is removed).
- [ ] **TESTS**: add a `DeviceCapabilities` unit/headless check (or a documented manual non-RT-GPU validation run) covering the timestamp-without-RT path.
- [ ] **UNSAFE**: the `reset_query_pool` call is `unsafe` — ensure the feature precondition is documented at the call site.
- [ ] **FFI / LOCK_ORDER / CANONICAL-BOUNDARY**: N/A.
