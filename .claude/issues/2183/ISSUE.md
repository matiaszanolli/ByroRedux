# 2183: PERF-D9-NEW-03: gpu_breakdown() omits the new upscale/presentation GPU-timer brackets

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/2183
**Labels**: bug, low, performance

---

## Severity
LOW

## Dimension
Telemetry (Dim 9) — `/audit-performance` 2026-07-25

## Location
`crates/renderer/src/vulkan/gpu_timers.rs` (`gpu_breakdown()`)

## Description
`gpu_breakdown()` — the SLOW-FRAME / 1 Hz diagnostic line — does not include the new `upscale` and `presentation` GPU-timer brackets added for FSR 3.1, so the reported per-phase breakdown no longer accounts for the whole frame's GPU time.

## Impact
The slow-frame diagnostic under-reports total GPU time by however long the upscale + presentation passes take, misleading anyone using it to attribute a frame-time spike to the wrong phase.

## Related
PERF-D9-NEW-04 (filed separately, same file, adjacent doc-rot); D5-N1 (filed separately, same "new FSR resource not reflected in accounting" class).

## Suggested Fix
Add `upscale` and `presentation` brackets to `gpu_breakdown()`'s reported phase list.

## Completeness Checks
- [ ] **TESTS**: Verify the breakdown sum equals total frame GPU time after the fix
