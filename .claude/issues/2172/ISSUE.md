# 2172: PERF-D1-02: collect_lights builds a fresh decorate-sort Vec every frame instead of a caller-owned scratch

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/2172
**Labels**: bug, low, performance

---

## Severity
LOW

## Dimension
CPU Hot Paths (Dim 1) — `/audit-performance` 2026-07-25

## Location
`byroredux/src/render/lights.rs` (`collect_lights`)

## Description
`collect_lights` allocates a fresh `Vec` each frame for its GI-priority decorate-sort instead of reusing a caller-owned scratch buffer, unlike the other Session-46 scratch-reuse guards in the same file family (`AnimScratch`, `drain_dirty_into`).

## Impact
Redundant allocation in a hot per-frame path. Low absolute cost (light counts are small relative to instance counts) but avoidable.

## Related
Session-46 scratch/gating guards (all re-verified intact this sweep).

## Suggested Fix
Thread a caller-owned scratch `Vec` through `collect_lights` and clear+reuse it, matching the pattern already established for `AnimScratch` and friends.

## Completeness Checks
- [ ] **TESTS**: Existing light-collection tests should still pass unchanged after the scratch-reuse refactor
