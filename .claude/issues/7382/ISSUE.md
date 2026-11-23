# #7382 — REN-D7-2026-11-23-01: PAF-012's tier-0 search shed has no pin on the dispatch site that actually sheds it

Snapshot of the issue as filed; GitHub is authoritative for current state.

Filed from `docs/audits/AUDIT_RENDERER_2026-11-23.md` (finding REN-D7-01). Reported by reading: nothing here was run on a GPU.

- **Severity**: LOW
- **Dimension**: Soft Shadows / Denoiser (test coverage)
- **Location**: `VulkanContext::record_shadow_filter` (`context/post_passes.rs`), `ShadowFilterPipeline::dispatch` (`shadow_filter.rs`), test `penumbra_search_is_shed_at_tier_zero_only` (`scene_buffer/ray_budget.rs`)
- **Status**: NEW
- **Description**: The skill (Dim 7 shadow-filter bullet) cites `penumbra_search_is_shed_at_tier_zero_only` as the pin for "the tier-0 dispatch passes `pixelAngle` 0". The test only evaluates `AdaptiveRayBudget::settings_for_tier(t).quality_tier >= SHADOW_FILTER_PENUMBRA_SEARCH_MIN_TIER` for t in 0..=3 and asserts `[false, true, true, true]`: it proves the comparison, not that the comparison is the argument `record_shadow_filter` hands to `dispatch(..., penumbra_search)`, nor that `dispatch` maps `false` to `pixel_angle = 0.0`. No other test names `SHADOW_FILTER_PENUMBRA_SEARCH_MIN_TIER` or the `penumbra_search` parameter (grep over crates/ and byroredux/ finds only the definition, the re-export, the call site and `dispatch`).
- **Evidence**: `record_shadow_filter` passes `budget.quality_tier >= crate::vulkan::scene_buffer::SHADOW_FILTER_PENUMBRA_SEARCH_MIN_TIER`; `dispatch` has `let pixel_angle = if penumbra_search { pixel_angle(self.fov_y_radians, self.height) } else { 0.0 };`. The shader's `pixelAngle <= 0` arm returns `pc.reachMax` (full reach, no Gaussian). Reading the code, all three links are correct at HEAD.
- **Impact**: None today. A later edit that passes `true` unconditionally (or the reverse) keeps the suite green and only shows up as a 0.24-0.31 ms tier-0 cost regression (A13 numbers in a2f3e9b69) or as a missing penumbra at tiers 1-3.
- **Related**: PAF-012 / #6603 (`a2f3e9b69`); sibling pin `the_shadow_filter_dispatch_and_the_composite_flag_share_one_predicate`.
- **Suggested Fix**: add a `production_text` source-shape test over `post_passes.rs` + `shadow_filter.rs` (the `penumbra_search` argument is the `quality_tier >= SHADOW_FILTER_PENUMBRA_SEARCH_MIN_TIER` expression; `dispatch` gates `pixel_angle(` on it), or fold the decision into one pure `fn penumbra_search_for_tier(u32) -> bool` that the test and the call site share.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other passes, other shaders, other call sites)
- [ ] **TESTS**: A regression test pins this specific fix
