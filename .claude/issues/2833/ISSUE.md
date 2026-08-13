# REN-D23-08: ExposureResource fallback mismatch — presentation defaults to 0.85, FSR substitutes 1.0

Labels: low, renderer, bug

## Description

On the happy path FSR and the tone mapper agree exactly (one `ExposureResource` feeds both, and the SDK's convention matches the shader's — both *multiply* scene colour). On the `ExposureResource::new` failure branch they fall back **independently**: presentation uses `DEFAULT_EXPOSURE` (0.85) while FSR receives a null resource and the SDK substitutes its internal default, whose accessor rewrites a zero texel to `1.0`. Reconstruction would then normalise against 1.0 while the tone mapper grades against 0.85 — a ~1.18× mismatch in the luma domain FSR uses for locking and history rectification. Only reachable if a 1×1 image allocation fails at startup. Worth naming because the fallback constant lives in two places with two different values and nothing ties them together.

## Location

`crates/renderer/src/vulkan/context/mod.rs` (the `ExposureResource::new` fallback), consumed by `record_upscale_pass` / `record_presentation_pass` in `crates/renderer/src/vulkan/context/post_passes.rs`

## Source

Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D23-08).

https://github.com/matiaszanolli/ByroRedux/issues/2833
