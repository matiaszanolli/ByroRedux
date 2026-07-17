# PERF-D1-2026-07-16-02: collect_lights recomputes gi_priority_score on both sides of every sort comparison

**Labels**: low, performance, bug

**Severity**: LOW
**Dimension**: CPU Hot Paths
**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-07-16.md`

## Location
`byroredux/src/render/lights.rs:206-207`

## Description
`collect_lights` sorts the point-light suffix by descending `gi_priority_score`, but the comparator recomputes `gi_priority_score` on both sides of every comparison rather than precomputing it once per element (e.g. via a Schwartzian transform / decorate-sort-undecorate). Compute-only (not allocation); point-light counts are small (streaming-RIS-capped, typically <50).

Verified current: `gpu_lights[directional_count..].sort_by(|a, b| gi_priority_score(b).total_cmp(&gi_priority_score(a)))` still recomputes the score function on both `a` and `b` for every comparison.

## Impact
Not worth a change unless a hundreds-of-lights cell ever materializes.

## Suggested Fix
Precompute `gi_priority_score` once per light into a parallel array/tuple before sorting, if this path is ever revisited for a larger light count.

## Completeness Checks
- [ ] **TESTS**: N/A — no functional change, informational optimization only
