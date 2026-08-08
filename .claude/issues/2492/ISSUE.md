# REN-D8-N02: CompositeParams::underwater and depth_params.y (exposure) are dead fields still documented as live

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2492
**Finding ID**: REN-D8-N02 (source: `docs/audits/AUDIT_RENDERER_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 8 — Denoiser/Composite
**Location**: `crates/renderer/src/vulkan/composite.rs:117-127` (`CompositeParams::underwater` doc), `crates/renderer/shaders/composite.frag:51-56` (UBO field comment), `crates/renderer/src/vulkan/context/draw.rs:574-594` (`build_composite_params`, `depth_params[1] = exposure_value`)
**Status**: NEW (doc-rot / dead plumbing)

## Description
`composite.frag` still declares `vec4 underwater;` in its UBO with a comment stating "The shader's final branch mixes `combined` toward `underwater.xyz` by a depth-driven extinction when `underwater.w > 0`." No such branch exists in `main()` any more — the underwater post-FX moved to `presentation.frag` with the output-resolution frame split, which `reflect.rs::composite_frag_spv_matches_recompiled_branch_count` explicitly records. The host-side field doc in `composite.rs` carries the same stale description, and `draw.rs` still uploads a live `underwater` value into the composite UBO *and* passes the same value to `record_presentation_pass`. `depth_params.y` (exposure) is the same shape: `build_composite_params` computes and uploads `exposure_value` with a comment claiming composite consumes it, but `composite.frag` only reads `depth_params.x` and `.w`; exposure is consumed by `presentation.frag`'s push constants.

## Evidence
`composite.frag` `main()` ends at `outColor = vec4(combined, direct4.a);` with no `params.underwater` reference; `grep 'depth_params' composite.frag` yields only `.x` (line 395) and `.w` (line 157). `presentation.frag:113` owns the live `params.underwater.w > 0.0` branch and `presentation.frag:111` the live `aces(graded * params.exposure)`.

## Impact
No runtime effect (16 wasted UBO bytes plus one f32). The risk is directional: a maintainer trusting either doc could "restore" the missing composite branch, producing a double underwater tint (once pre-tone-map in composite, once post-tone-map in presentation) or a double exposure multiply.

## Related
`reflect.rs::composite_frag_spv_matches_recompiled_branch_count` (#1917) is the only place that records the move correctly.

## Suggested Fix
Either drop `underwater` / the exposure slot from `CompositeParams` (and the matching GLSL fields + `build_composite_params` plumbing), or rewrite both doc blocks to say "reserved — the live consumer is `presentation.frag`". Note dropping the field changes the UBO block size, so the `composite_params_is_16_byte_aligned_std140_shape` test and the `.spv` need a coordinated recompile.

## Completeness Checks
- [ ] **TESTS**: If the field is dropped, `composite_params_is_16_byte_aligned_std140_shape` is updated and `.spv` recompiled in lockstep
