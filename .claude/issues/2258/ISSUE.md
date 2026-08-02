# TD1-080: record_post_passes is a 556-LOC single function covering 8+ distinct GPU passes inline, zero test coverage

Severity: low
Source audit: docs/audits/AUDIT_TECH_DEBT_2026-08-02.md
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2258

**Dimension**: 1 (File/Function/Module Complexity)
**Location**: `crates/renderer/src/vulkan/context/post_passes.rs:137-693` (`record_post_passes`, ~556 LOC of the file's 696 total)
**Status**: NEW

**Description**: `post_passes.rs` was split out of `context/mod.rs`/`draw.rs` under #1857 specifically to isolate post-geometry pass recording — that file-level split succeeded, but inside it, `record_post_passes` itself was never further decomposed. It's one function that inline-records the water-caustic barrier, SVGF temporal+spatial passes, SSAO, bloom down/up pyramid, volumetrics inject/integrate dispatch (now also carrying the shadow-policy-aware TLAS shadow ray), composite, TAA, FSR upscale, and presentation passes back-to-back, each gated by its own permanent-failure latch and GPU-timer bracket. The file has only one other method (`copy_depth_to_history`, ~100 LOC) and zero test coverage (no `#[cfg(test)]` block at all).

**Evidence**: `grep -n "fn " crates/renderer/src/vulkan/context/post_passes.rs` → only `copy_depth_to_history` (line 32) and `record_post_passes` (line 137, closing near line 693/696 EOF).

**Impact**: Maintainability only; not proposing any reordering of the passes themselves — Vulkan-recording splits here are render-pass-adjacent and any barrier/order change needs RenderDoc, not a file-organization pass. A per-pass-group split would only move code, not reorder execution.

**Related**: Existing #1857 (CLOSED — file-level split that created this file); this is the natural next-level split the closed issue didn't reach.

**Suggested Fix**: Extract each self-contained pass block (SSAO, bloom, volumetrics-dispatch, composite, TAA, FSR-upscale, presentation) into its own `pub(super) fn record_<pass>_pass(&mut self, cmd, frame, ...)` private helper called in sequence from `record_post_passes`, mirroring the boundary already drawn between `geometry_pass.rs`/`skinned_blas_refit.rs`/`post_passes.rs` themselves at the file level — same axis, one level deeper. Purely a call-order-preserving decomposition; do not reorder barriers or passes while doing it. Pair with a `cargo run` + validation-layer smoke check, not verifiable from `cargo test` alone since there's no test module in this file today.

## Completeness Checks
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix, if applicable
