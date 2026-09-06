# REN-WD-D1-01: STATIC_BLAS_FLAGS doc + build_blas_batched comment still name the deleted single-shot build_blas

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
- [x] **SIBLING**: Grep the rest of `acceleration/` for other stale references to `build_blas` (single-shot) before closing

## Resolution
Fixed both comments named in the issue, plus three more stale references the SIBLING sweep turned up:
- `constants.rs`'s `STATIC_BLAS_FLAGS` docstring — now names only `build_blas_batched`, notes the deletion.
- `blas_static.rs`'s `#2692` pre-batch eviction comment — dropped the dangling "as at the single-shot site above".
- `mod.rs`'s `pending_destroy_scratch` docstring — "add via `build_blas` / `build_blas_batched`" listed a mutation site that no longer exists.
- `blas_static.rs:555` (`#1782`, scratch-buffer deferred-destroy) — "see the matching comment in `build_blas` above" pointed at nothing; redirected to the real still-existing sibling, `memory::shrink_blas_scratch_to_fit`.
- `blas_static.rs:951` (`#2481`/AS-D1-NEW-02, drop-before-overwrite guard) — same dangling pattern; redirected to the real sibling, `blas_skinned.rs`.
- `blas_static.rs:1068` (`evict_unused_blas`'s call-site list) — named `build_blas`/`draw.rs`; neither is current (real callers are `build_blas_batched`'s three internal sites plus `dispatch_skin_and_cluster.rs`, post-#3282 draw.rs split).

Left alone: `blas_static.rs:217` ("eliminates the per-mesh fence stall from `build_blas`") — a legitimate historical comparison explaining what problem `build_blas_batched` solves, not a claim that `build_blas` still exists. `tests/blas_static_tests.rs:18-19` was already correctly phrased as historical.

`cargo check -p byroredux-renderer` clean (comment-only diff).
