# PERF-D9-01: GPU timer brackets use TOP_OF_PIPE start, risking pass-cost misattribution

**Labels**: low, performance, renderer, bug

**Severity**: LOW
**Dimension**: Telemetry & Origin Cost
**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-07-16.md`

## Location
`crates/renderer/src/vulkan/gpu_timers.rs:355-760`

## Description
Diagnostic-accuracy-only: a bracket's reported ms can absorb queue-wait from prior in-flight work because timer brackets use `vk::PipelineStageFlags::TOP_OF_PIPE` as the start stage, so per-bracket timings are an upper bound and must not be summed to a "total GPU ms" without caveat.

Verified current: `crates/renderer/src/vulkan/gpu_timers.rs` still writes timestamps with `vk::PipelineStageFlags::TOP_OF_PIPE` at all cited call sites (lines ~365, 402, 434, 470).

## Impact
Diagnostic-accuracy-only — doesn't change actual GPU cost, only how it's reported/interpreted in telemetry.

## Suggested Fix
Document the caveat prominently near `GpuTimerSnapshot`'s definition (per-bracket timing is an upper bound, not directly summable), or switch to a later pipeline stage per bracket if tighter attribution is needed.

## Completeness Checks
- [ ] **TESTS**: N/A — diagnostic-accuracy documentation issue, no functional fix required
