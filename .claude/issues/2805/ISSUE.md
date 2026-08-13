# REN-D16-06: BLOOM_INTENSITY has two contradictory documented derivations

Labels: low, renderer, documentation

## Description

`BLOOM_INTENSITY = 0.15` carries **two mutually exclusive documented derivations** — one says it absorbs the un-normalised 5× DC gain relative to Frostbite's 0.04, the other says it compensates LDR-authored Bethesda content; the 4× factor is spent once in each comment on a different justification, and absorbing a 5× gain against 0.04 would require ≈ 0.008. Measurable consequence: the effective DC weight is **0.75×** the local blurred average (~19× the normalised reference), and `bloom_downsample.comp` applies **no bright-pass threshold or Karis average** (`DownsampleParams` carries only `inv_resolutions`), so this is a broadband lift, not a highlight-only glow. Filed as a documentation contradiction plus a quantified observation — **not** a claim the image is wrong.

## Location

`shader_constants_data.rs`, `crates/renderer/src/vulkan/bloom.rs`, `crates/renderer/shaders/bloom_upsample.comp`, `crates/renderer/shaders/bloom_downsample.comp`

## Source

Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D16-06).

https://github.com/matiaszanolli/ByroRedux/issues/2805
