# 1857: TD1-001: context/draw.rs is 4265 LOC with a 1844-LOC draw_frame

https://github.com/matiaszanolli/ByroRedux/issues/1857

Labels: bug, renderer, low, tech-debt

**Severity**: LOW · **Dimension**: 1 (Complexity)
**Location**: `crates/renderer/src/vulkan/context/draw.rs:1992-3836` (`draw_frame`, ~1844 LOC), `:784-1404` (`record_geometry_pass`, ~620 LOC), `:1404-1992` (`record_skinned_blas_refit`, ~588 LOC)
**Status**: NEW (largest file; grew after the Session-34/35 splits closed the original set)
**Audit**: docs/audits/AUDIT_TECH_DEBT_2026-07-02.md (TD1-001)

## Description
`context/draw.rs` is the largest file in the tree at 4265 LOC. `#1748` (CLOSED) previously
addressed a 3325-LOC `draw_frame` and that fix held — `draw_frame` is now ~1844 LOC — but the
file has since grown around it: two record helpers (`record_geometry_pass` ~620 LOC,
`record_skinned_blas_refit` ~588 LOC) add roughly 1200 more LOC of per-frame command-recording
code alongside it. Every per-frame renderer edit, review, and merge pays a tax here.

## Evidence
`awk` fn-boundary scan — `draw_frame` @1992→3836; `record_geometry_pass` @784;
`record_skinned_blas_refit` @1404. First `#[cfg(test)]` at 3852.

## Impact
Highest-leverage complexity debt; not a correctness bug.

## Suggested Fix
**This is Vulkan command-recording code** — per `feedback_speculative_vulkan_fixes.md`, split by
*extracting cohesive recording blocks into `&self` helpers*, NOT by reordering
barriers/passes. Suggested axis: acquire/sync → geometry-pass record → RT/BLAS refit →
post-passes → submit. Extract the post-pass sequence (already isolated as
`record_post_passes` @415) further, and lift the geometry-record and skinned-refit blocks
that `draw_frame` inlines into named `record_*` helpers so `draw_frame` becomes an
orchestrator. Effort: large (decompose first).

## Related
Distinct from `#1749` (TD1-004, `VulkanContext::new()` / `context/mod.rs`) and the now-fixed
`#1748` (`draw_frame` complexity, CLOSED — this finding is the file growing back around the
fix, not a regression of the original function).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix

