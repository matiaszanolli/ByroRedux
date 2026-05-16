# Issue #1133 — PERF-D7-NEW-01: skin-path scratch cluster

**Source**: AUDIT_PERFORMANCE_2026-05-16
**Severity**: MEDIUM (perf)
**Status**: CLOSED in 4f55b2f1

## Resolution

3 per-frame allocs in skinned dispatch walker moved into `*_scratch` cluster on `VulkanContext`:
- `skin_dispatch_seen_scratch`
- `skin_dispatches_scratch`
- `skin_first_sight_builds_scratch`

`fill_scratch_telemetry` updated with 3 new rows.
