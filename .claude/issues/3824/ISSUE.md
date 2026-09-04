# Issue #3824: REN-WD-D1-01: STATIC_BLAS_FLAGS doc + build_blas_batched comment still name the deleted single-shot build_blas

**Labels**: low,renderer,documentation,doc-rot
**Filed**: 2026-09-04, via /audit-publish from the water-deep audit suite

---

**Severity**: LOW
**Dimension**: AS Correctness (doc-rot)
**Location**: `crates/renderer/src/vulkan/acceleration/constants.rs` (`STATIC_BLAS_FLAGS` docstring), `crates/renderer/src/vulkan/acceleration/blas_static.rs` (pre-batch eviction comment inside `build_blas_batched`, `#2692 — as at the single-shot site above`)
**Source report**: `docs/audits/AUDIT_RENDERER_2026-09-04.md` (water-deep suite, Dim 1)

## Description
`#2914` deleted the never-called single-shot `build_blas` / `build_blas_for_mesh` pair and updated `docs/engine/memory-budget.md`, but two in-code doc comments were not updated. `STATIC_BLAS_FLAGS`'s docstring documents "the static-BLAS BUILD call sites in `blas_static.rs` (`build_blas` single-shot plus `build_blas_batched` per-mesh size-query and per-mesh record)" — three sites where two exist. `build_blas_batched`'s pre-batch eviction comment opens "`#2692` — as at the single-shot site above", pointing at a site that no longer exists anywhere in the file.

## Evidence
`grep -rn "fn build_blas" crates/renderer/src/` returns only `blas_static.rs::build_blas_batched` and its `context/resources.rs` wrapper; `crates/renderer/src/vulkan/acceleration/tests/blas_static_tests.rs` records "`#2914` deleted the third — the never-called single-shot".

## Impact
Documentation only. The concrete cost is auditor time: the VUID-03801 "all static-BLAS sites must share the flag constant" invariant is stated against a site count that no longer matches the code — exactly the class of drift that produced the false Dimension-1 premise in #3576.

## Related
#2914, #1892, #3576.

## Suggested Fix
Rewrite both comments to name only `build_blas_batched`'s size-query and record sites.

## Completeness Checks
- [ ] **SIBLING**: Grep the rest of `acceleration/` for other stale references to `build_blas` (single-shot) before closing
