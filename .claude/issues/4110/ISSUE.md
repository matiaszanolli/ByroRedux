# REN-2026-09-11-D11-01: water.rs and presentation.rs build descriptor-set layouts with zero reflect::validate_set_layout coverage

**Issue**: https://github.com/matiaszanolli/ByroRedux/issues/4110
**Labels**: bug, renderer, medium, pipeline

**Severity**: MEDIUM
**Dimension**: Pipeline/RenderPass
**Location**: `crates/renderer/src/vulkan/water.rs` (`WaterPipeline::new`, the `water_caustic_set_layout` construction); `crates/renderer/src/vulkan/presentation.rs` (`PresentationPipeline`'s `descriptor_set_layout` construction); `crates/renderer/src/vulkan/reflect.rs` (`validate_set_layout`)
**Status**: NEW (from `docs/audits/AUDIT_RENDERER_2026-09-11.md`)

## Description
Every other pipeline that owns a descriptor-set layout (`bloom.rs`, `caustic.rs`, `composite.rs`, `compute.rs`, `groundcover.rs`, `skin_compute.rs`, `ssao.rs`, `svgf.rs`, `taa.rs`, `volumetrics/init.rs`, plus the two shared set-0/set-1 layouts) calls `reflect::validate_set_layout` at construction to catch drift between the hand-written Rust `DescriptorSetLayoutBinding` list and the SPIR-V that actually declares those bindings. `water.rs`'s set 2 (water-caustic accumulator + `GpuWaterParams[]`) and `presentation.rs`'s set 0 (upscaled-scene sampler + image-health counter) build their layouts with only a prose comment asserting the binding matches — no test would catch drift.

## Evidence
Confirmed by direct read: `grep -rln validate_set_layout crates/renderer/src/vulkan/*.rs` lists `compute.rs, composite.rs, caustic.rs, groundcover_bench.rs, ssao.rs, reflect.rs, bloom.rs, taa.rs, skin_compute.rs, svgf.rs, volumetrics.rs` — `water.rs` and `presentation.rs` are absent from that list, though both call `create_descriptor_set_layout` directly (`water.rs:356`, `presentation.rs:237`). Both layouts are currently correct by hand-inspection against `water.vert`/`water.frag` and `presentation.frag` respectively.

## Impact
Defense-in-depth gap, same class as the already-tracked `REN-2026-09-06-D11-01`/#3977 (the fragment-output-interface sibling of this exact gap class — that finding's own suggested fix named this generalization, which was never implemented). A future binding edit to either shader that isn't mirrored in the Rust layout fails only at `vkCreateDescriptorSetLayout`/`vkUpdateDescriptorSets` time (or silently misbehaves with validation off), invisible to `cargo test`. Not a live bug today.

## Related
`REN-2026-09-06-D11-01` / #3977

## Suggested Fix
Add a `reflect::validate_set_layout` call at the end of each pipeline's layout construction, mirroring the pattern `scene_buffer/buffers.rs` already uses for set 1. Pure test-time reflection check; no Vulkan behavior change.

## Completeness Checks
- [ ] **SIBLING**: Same pattern (`validate_set_layout` call at layout construction) already exists in 11 other pipeline files — apply identically to `water.rs` and `presentation.rs`
- [ ] **TESTS**: A regression test pins that both layouts now call `validate_set_layout`
