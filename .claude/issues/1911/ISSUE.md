# REN-D1-01: memory-budget.md misattributes shrink_tlas_to_fit to cell-unload; live call site is per-frame end-of-frame in draw_frame

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/1911

**Severity**: low
**Dimension**: renderer audit 2026-07-09
**Location**: `docs/engine/memory-budget.md` (Acceleration Structures → Scratch buffers) vs `crates/renderer/src/vulkan/context/draw.rs:3976-3997` + `byroredux/src/cell_loader/unload.rs:141`
**Status**: NEW

## Description
The authoritative memory doc states "`shrink_blas_scratch_to_fit` and `shrink_tlas_to_fit` run at cell-unload time to reclaim VRAM after a peak scene is evicted." Only the first half is true. `shrink_blas_scratch_to_fit` runs at cell-unload and on swapchain recreate. `shrink_tlas_to_fit` and `shrink_tlas_scratch_to_fit` run at the **end of every `draw_frame`**, targeting the just-incremented FIF slot whose fence was waited at frame start — a different site with a different (and stricter) safety precondition. The doc's LRU section likewise omits the per-frame post-TLAS `evict_unused_blas` call and the single-shot `build_blas` guard.

## Evidence
`grep shrink_tlas_to_fit` yields exactly one production call site, `draw.rs:3980`, inside `draw_frame` after `current_frame` increments; `unload.rs` calls only `shrink_blas_scratch_to_fit`.

## Impact
Documentation only. An engineer adding a new TLAS-slot teardown path and trusting the doc could copy the "cell-unload" placement — exactly the in-flight window where immediate TLAS/instance-buffer destroys would be a GPU use-after-free (the #1782 class of bug).

## Related
#1782 (why the unload window is dangerous), #1226 (TLAS-scratch shrink)

## Suggested Fix
Reword the sentence: BLAS scratch shrink → cell-unload + resize; TLAS instance/scratch shrink → end-of-frame in `draw_frame` (post fence-wait, slot-rotated). Add the two missing `evict_unused_blas` call sites to the LRU section.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
