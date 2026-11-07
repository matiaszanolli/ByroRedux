# REN-D7-02: two CPU-decides / GPU-consumes agreements in the resolve chain have no pin (shadow-filter flag, composite `LOCAL_FIRE_SUNLIT` specialization)

Labels: low,renderer,test-gap,bug
URL: https://github.com/matiaszanolli/ByroRedux/issues/6748

**Severity**: LOW · **Dimension**: Denoiser/Composite

## Location
`VulkanContext::shadow_filter_active` (`crates/renderer/src/vulkan/context/post_passes.rs`) vs the `shadow_filter_active` composite input (`context/build_and_upload_instances.rs`, lands in `caustic_flags.y`); `CompositePipeline::dispatch(.., local_fire_sunlit)` (`crates/renderer/src/vulkan/composite.rs`) vs `LOCAL_FIRE_SUNLIT` in `crates/renderer/shaders/composite.frag`

## Description
(a) The dispatch (`record_shadow_filter`) and composite's flag both call `shadow_filter_active()`, and `r.shadowfilter` flips `shadow_filter_enabled` only at a frame boundary, so they agree today; no test fixes it (the audit skill already flags this as unpinned). (b) Pipeline creation hard-codes `constant_id(0)` data while the GLSL declares `layout(constant_id = 0) const bool LOCAL_FIRE_SUNLIT`; `composite_frag_spv_matches_recompiled_branch_count` (161) sees only the unspecialised SPIR-V and nothing asserts the id / semantics match or that both pipelines are created and destroyed.

## Evidence
`grep shadow_filter_active` shows exactly two consumers (`record_shadow_filter`, the composite input); `grep LOCAL_FIRE_SUNLIT|constant_id` in `composite.rs` shows no scanning test. The CPU choice is `GpuLocalGridParams::sunlit` (set in `write_params`, cleared in `clear_params`).

## Impact
A refactor that gates the dispatch on something the composite flag does not see, or renumbers the spec constant, would silently read a stale / fallback correction view or drop sun scatter from the close fire march, with no failing test.

## Suggested Fix
Two `source_scan::production_text` tests: both consumers use `shadow_filter_active()` and no other predicate; `constant_id(0)` in `composite.rs` pairs with `constant_id = 0` / `LOCAL_FIRE_SUNLIT` in `composite.frag`, and `pipeline_sunless` is created and destroyed.

Source: `docs/audits/AUDIT_RENDERER_2026-11-07.md` (delta audit against baseline 670f81917, HEAD 9409e7b20).

## Completeness Checks
- [ ] **SIBLING**: Neighbouring unpinned contracts checked
- [ ] **TESTS**: The new pin fails when the wiring is removed (mutation-check it)
