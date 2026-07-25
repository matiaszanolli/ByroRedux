# 2184: PERF-D9-NEW-04: gpu_timers.rs doc comments still say 12 brackets / 24 queries; actual is 14 / 28 since the FSR brackets landed

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/2184
**Labels**: bug, low, performance

---

## Severity
LOW

## Dimension
Telemetry (Dim 9) — `/audit-performance` 2026-07-25

## Location
`crates/renderer/src/vulkan/gpu_timers.rs` (doc comments)

## Description
`gpu_timers.rs`'s doc comments still cite "12 brackets / 24 queries" — the FSR 3.1 work added `upscale` and `presentation` brackets (each contributing a begin/end query pair), making the actual current count 14 brackets / 28 queries.

## Impact
Doc-rot only; a maintainer adding a 15th bracket would miscalculate the expected query-pool size from the stale comment.

## Related
PERF-D9-NEW-03 (filed separately, same file, same missing brackets in `gpu_breakdown()`).

## Suggested Fix
Update the doc comments to 14 brackets / 28 queries, confirmed against the current bracket list.

## Completeness Checks
- [ ] N/A — documentation-only fix
