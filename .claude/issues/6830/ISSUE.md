# REN-D4-02: the #4773 positional-`f32` pin covers `record_post_passes` only; the `build_and_upload_instances` call, where #6728 threaded `fog_coverage`, is unpinned

Labels: low,renderer,test-gap,bug
Filed from: volumetrics-deep audit suite, 2026-11-09

**Severity**: LOW (test-gap). **Status**: NEW.

**Location**: `crates/renderer/src/vulkan/context/draw.rs` (the `self.build_and_upload_instances(` call, `fog_scale_height_meters, fog_coverage,` on one line, ~L273) and the matching signature in `context/build_and_upload_instances.rs`; test `record_post_passes_named_arguments_match_parameter_positions` (`context/post_passes.rs`).

## Description
#4773 was a swap of two adjacent `f32` arguments (`fog_coverage` / `fog_scale_height_meters`) that compiled and collapsed the froxel medium to a 1 m layer. The pin it left splits `record_post_passes`'s call one argument per line and checks names against positions. `3500d2737` (#6728) added `fog_coverage` to the sibling call, which now has adjacent `f32` fog lanes (`fog_single_scatter_albedo`, `fog_scale_height_meters`, `fog_coverage`, `fog_clip`, `fog_power`, `fog_height_reference`) and puts two arguments on one line, which the pin's parser could not read anyway. Today the order matches (checked by hand against the signature).

## Impact
A future swap of the new lane with its neighbour feeds `fog_mean_density_scale` a scale height (or the scale height a coverage) and silently changes the whole beyond-grid fog.

## Suggested Fix
One argument per line at both sites and extend the existing test to also walk the `build_and_upload_instances` call (reuse its helper); or pass a `FogParams` struct built by name.

Report: `docs/audits/AUDIT_RENDERER_2026-11-09.md`.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix

