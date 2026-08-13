# REN-D14-NEW-01: INSTANCE_FLAG_CAUSTIC_SOURCE set on draws the splat shader unconditionally rejects (opaque/alpha-tested glass, opaque MultiLayerParallax)

- **Severity**: MEDIUM
- **Dimension**: 14 — Caustics. See Cluster B, Mechanism 2.
- **Location**: `crates/renderer/src/vulkan/context/draw.rs` (`is_caustic_source`, `is_refractive_glass`, and the `f |= INSTANCE_FLAG_CAUSTIC_SOURCE` site) vs. `crates/renderer/shaders/caustic_splat.comp` (the bit-31 mesh-ID gate) and `crates/renderer/shaders/triangle.frag` (the `outMeshID` write)
- **Description**: The CPU gate and the GPU gate disagree about what a caustic source is. `is_caustic_source` consults only `material_kind` and `multi_layer_refraction_scale` — never `alpha_blend`. The shader rejects every pixel whose mesh-ID bit 31 is clear, and that bit is written from `INSTANCE_FLAG_ALPHA_BLEND` alone. Any caustic-source instance that is not alpha-blended carries the flag and can never splat. The shader's own comment asserting the opposite ("caustic sources are always alpha-blend") is wrong.
- **Evidence**: `MATERIAL_KIND_GLASS` is assigned via `classify_glass_into_material` gated on `has_transparent_coverage`, fed `source.has_alpha || source.alpha_test` — deliberately, per the classifier's own doc: "Alpha-tested glass is deliberately allowed". MultiLayerParallax (kind 11) is accepted on `multi_layer_refraction_scale > 0.0` with no transparency condition at all. The Cornell harness is 100% affected: `cornell.rs`'s `glass()` probes state "Glass is OPAQUE (no AlphaBlend)". Every fixture in `is_caustic_source_tests` is hard-coded `alpha_blend: true`.
- **Impact**: Silent, permanent loss of caustics for an entire content class — alpha-test broken panes, opaque MLP ice/glass — with no log, telemetry or test. Makes the Cornell harness unusable as a caustic reference. The bit-31 gate itself is correct and must stay (opaque pixel's low bits are `surfaceId`, not an instance index).
- **Related**: #922 (the CPU gate tightening that introduced the asymmetry), #2515, REN-D11-2026-08-12-01 (compounds — fixing either alone leaves the pass dark), REN-D21-01.
- **Suggested Fix**: Make `is_caustic_source` require `cmd.alpha_blend` so the flag stops lying, add the `alpha_blend: false` case to `is_caustic_source_tests`, correct the shader comment. If opaque glass should cast caustics, that's a larger separate enhancement.

## Completeness Checks
- [ ] SIBLING: `needs_two_sided_blend_split` shares `is_refractive_glass`
- [ ] TESTS: Add `alpha_blend: false` case to `is_caustic_source_tests`; fix Cornell `glass()` probe reachability

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2765
