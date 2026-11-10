# PERF-2026-11-09-D5-01: the composite pipeline pair is specialised on sun radiance only, so every daytime exterior frame uses the 126-register variant although no fire march runs without a live box (unmeasured)

Labels: low,renderer,shaders,performance,bug
Filed from: volumetrics-deep audit suite, 2026-11-09

**Severity**: LOW — **unmeasured** (filed as a measure-first item). **Status**: NEW.

**Location**: `CompositePipeline::dispatch` / `pipeline_sunless` (`crates/renderer/src/vulkan/composite.rs`), `GpuLocalGridParams::sunlit` and `new` (`crates/renderer/src/vulkan/volumetrics/local_grid.rs`; `sun_radiance` is filled outside the `if simulated` block), `LOCAL_FIRE_SUNLIT` (`crates/renderer/shaders/composite.frag`), the pipeline pick from `local_fire_sunlit()` in `record_post_passes`.

## Description
`15a4ecf10` made the march loop cheaper by specialising the pipeline on `LOCAL_FIRE_SUNLIT`; the recorded reason is register pressure: 96 registers per thread sun-less vs 126 sun-lit (`VK_KHR_pipeline_executable_properties`, 4070 Ti, `docs/engine/fire-volumetrics-plan.md`), costing ~0.1 ms of composite per scene "merely by existing in the loop". The selector is `sunlit() = any(sun_radiance > 0)`, and `sun_radiance` is set whether or not a box is live. So an exterior by day with no fire in reach (`localGrid.info.z == 0`, the march branch never taken) still binds the 126-register pipeline for the whole fullscreen composite. Register count is a property of the pipeline, not of the path taken.

## Evidence
`fn sunlit(&self) -> bool { self.sun_radiance[..3].iter().any(|&c| c > 0.0) }`; `params.sun_radiance = [frame.sun_color[0..3], 0.0]` is assigned outside `if simulated { .. }`. The plan doc's measurements are all scenes with fire in view; no no-fire frame is on record against a march-free build.

## Impact
Not quantified. Potential is the occupancy of the non-fire composite work (sky, fog compose, caustics, underwater branch) on the commonest frame type; upper bound by the doc's figure ~0.1 ms per scene at 720p, scaling with render pixels if occupancy-bound.

## Measure first
A/B `composite_ms` on a no-fire exterior (FNV Goodsprings noon, Skyrim Whiterun exterior) between the shipped build and one with the march folded out by a third specialization constant (`LOCAL_FIRE_ENABLED`, false when `info.z == 0`); read register counts with `VK_KHR_pipeline_executable_properties`. Interleave runs, pin `--rt-test-ray-quality-tier`. `BYRO_LOCAL_GRID_DEBUG=no_march` does not answer it: it is a runtime branch and leaves the register count alone.

## Suggested Fix (only if the A/B shows a gain)
A second constant and three pipelines (or two, selected by `live_mask != 0 && sunlit`), all in the same `create_graphics_pipelines` batch.

Report: `docs/audits/AUDIT_PERFORMANCE_2026-11-09.md`.

## Completeness Checks
- [ ] **MEASURED**: Before/after numbers from an interleaved A/B (tier pinned) are attached; close with the number if the delta is below noise
- [ ] **SIBLING**: Same pattern checked in related files (the `LOCAL_FIRE_SUNLIT` pair-validity test `the_sunless_composite_pipeline_is_the_same_shader_with_the_constant_false`)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct

