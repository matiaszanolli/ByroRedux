# REN-D10-NEW-01: #2240's freqScale multiplies water's absolute textured wave UV, amplifying the one un-rebased large-world consumer

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2496
**Finding ID**: REN-D10-NEW-01 (source: `docs/audits/AUDIT_RENDERER_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 10 — Camera-Relative Precision
**Location**: `crates/renderer/shaders/water.frag:221` (`sampleScrollingNormal`, textured branch); scale sourced at `:415`
**Status**: NEW (introduced by `6d40f6bf` / #2240, landed 2026-08-05, after the 2026-08-03 audit)

## Description
`sampleScrollingNormal` has two branches. The procedural branch was fixed under #1997 to rebase its hash input origin-relative: `vec2 uv = (uvBase - originOffset) * scale + scroll * time;` (relative, correct). The textured branch deliberately stays **absolute** so the wrapping sampler has no seam at a render-origin crossing: `vec2 uv = uvBase * scale * freqScale + scroll * time;` (absolute). #2240 inserted `freqScale = push.misc.y / 0.6` (WATR-authored `wave_frequency`, **unclamped**) into that product. `uvBase` is `vWorldPos.xz`, up to ~176k on MarkarthWorld. With the default `uv_scale_a = 1/256` and the default `wave_frequency = 0.6` (`freqScale == 1.0`) nothing changes from the pre-#2240 magnitude (~687, f32 ULP ≈ 6.1e-5 ≈ 1/16 texel on a 1024² normal map). But any WATR authoring `wave_frequency > 0.6` scales the UV magnitude — and therefore the quantization step — proportionally, with no upper bound. At `freqScale ≈ 3.3` the ULP reaches ~1/4 texel.

## Evidence
`:415` `float freqScale = push.misc.y / 0.6;` (no clamp) feeding `:221`. The companion in-code precision comment at `:183-193` documents the hazard for the procedural branch only and explicitly says the textured branch keeps its "absolute (wrapping) UV".

## Impact
Visual only, and only for textured water (Skyrim/FO4 WATR with a bound normal map) in a worldspace far from the origin *and* with an authored `wave_frequency` above the 0.6 default — the wave normal map stair-steps/aliases instead of resolving smoothly. Invisible near the origin and unreachable from `cargo test`; needs a large-world capture to confirm the practical magnitude. I did **not** verify what vanilla Skyrim actually authors for `wave_frequency`, so the real-content blast radius is unconfirmed — reporting the mechanism, not a claimed observed artifact.

## Related
#1997 (procedural-branch rebase), #2240 / `6d40f6bf` (the `freqScale` addition), #1502 (original water precision bound).

## Suggested Fix
Subtract the *tile-integral* part of the origin so the wrapping sampler is unaffected but the magnitude collapses: `vec2 o = floor(originOffset * scale * freqScale); vec2 uv = uvBase * scale * freqScale - o + scroll * time;`. Separately consider clamping `freqScale` to a sane authored range at the CPU packing site (`byroredux/src/render/water.rs:107`).

## Completeness Checks
- [ ] **TESTS**: A large-world capture confirms the practical magnitude before/after the fix (no game data → document as needs-verification)
