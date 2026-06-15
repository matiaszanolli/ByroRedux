# Issue #1636 — REG-04: host_query_reset capability gating (#1478) has no DeviceCapabilities test

_Snapshot as filed (immutable). GitHub is authoritative for current state._

**Source:** `docs/audits/AUDIT_REGRESSION_2026-06-14.md` — REG-04 (PARTIAL hardening gap, LOW)

The fix for **#1478** is **present and correct**; this issue tracks the missing guard test, not a regression.

## Description
`hostQueryReset` is now probed independently of ray-query support, and the GPU-timer reset gates on `timestamp_supported && host_query_reset_supported`. No `DeviceCapabilities` unit test was added (the original issue listed it as a checkbox).

## Evidence
- `crates/renderer/src/vulkan/device.rs:104` — `pub host_query_reset_supported: bool`
- `crates/renderer/src/vulkan/device.rs:297` — `let host_query_reset_supported = vulkan12_features.host_query_reset == vk::TRUE;` (probed independently of ray-query)
- `crates/renderer/src/vulkan/gpu_timers.rs:195` — `if !caps.timestamp_supported || !caps.host_query_reset_supported { … }`

## Impact
A regression would re-arm a host `vkResetQueryPool` without the feature enabled on RT-absent devices → Vulkan validation error (HIGH if it regressed; the coverage gap itself is LOW).

## Suggested Fix
Add a small unit test on the capability struct's gating logic: assert the GPU-timer reset path is taken only when `timestamp_supported && host_query_reset_supported`, and skipped when either is false.

## Completeness Checks
- [ ] **SIBLING**: Any other `host_query_reset`-dependent path gates on the same capability flag
- [ ] **TESTS**: A regression test pins the `timestamp_supported && host_query_reset_supported` gate
