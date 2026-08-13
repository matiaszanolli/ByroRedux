# REN-D15-02: water-side caustic deposits are single-pixel and cleared every frame, unlike the glass path's 5x5 Gaussian + decay

- **Severity**: MEDIUM
- **Dimension**: 15 — Water
- **Location**: `crates/renderer/shaders/water.frag` (`main`, the `imageAtomicAdd` tail); `crates/renderer/src/vulkan/water_caustic.rs` (`WaterCausticAccum::clear_pre_render_pass`); consumed by `crates/renderer/shaders/composite.frag` (the `causticRadiance` block)
- **Description**: The glass-side writer spreads each deposit over a 5×5 normalised Gaussian footprint and, when the camera is parked, runs a decay/EMA pass instead of a clear — its own comment states this is its only smoothing since compositing happens after TAA. The water-side writer does neither: deposits into exactly one pixel, and `clear_pre_render_pass` zeroes the whole per-FIF accumulator unconditionally every frame with no parked-camera decay branch. `composite.frag` sums both accumulators into the same term with no filtering of its own.
- **Evidence**: `caustic_splat.comp` runs a 5×5 `kGauss5`-weighted loop with a bounds guard; `water.frag` does a bare `imageAtomicAdd(waterCausticAccum, pixel, fixed_val);`. `clear_pre_render_pass` is the only per-frame state op on the water accumulator — unconditional clear, no decay branch.
- **Impact**: The water half of the shared caustic term reads as salt-and-pepper speckle rather than a focused pool, shimmering frame-to-frame since nothing downstream averages it. Exterior sunlit water only. Visual-quality only.
- **Related**: #2468 (OPEN — parked-camera caustic EMA has no dynamic-scene invalidation, concerns the glass decay branch which water lacks entirely); #1210 Phase D / #1256.
- **Suggested Fix**: Give the water deposit the same normalised footprint the glass path uses (share `kGauss5` + bounds guard via `crates/renderer/shaders/include/`). Decide separately whether the parked-camera decay branch should also apply to water.

## Completeness Checks
- [ ] SIBLING: caustic_splat.comp's kGauss5 footprint and decay branch, to be shared not duplicated
- [ ] TESTS: A regression test pins this specific fix; needs visual/RenderDoc confirmation since quality-only

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2789
