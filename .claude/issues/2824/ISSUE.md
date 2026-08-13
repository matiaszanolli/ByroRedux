# REN-D23-03: record_bloom_pass runs pre-upscale, contradicting fsr3-upscaler-integration-plan.md's frame graph

Labels: low, renderer, documentation

## Description

The plan's target frame graph says bloom and presentation post-processing consume the upscaled image at output resolution, and its status header names exactly **three** carried items. Bloom is a fourth: it still runs before composite/upscale and samples the raw pre-TAA render-extent HDR, so it enters FSR as part of scene colour and is temporally reconstructed with everything else. No runtime hazard (the pyramid is mip-relative, so the halo's output-relative radius is preserved); the cost is doc rot in the authoritative subsystem document.

## Location

`crates/renderer/src/vulkan/context/post_passes.rs` (`record_bloom_pass` + `record_post_passes` order) vs. `docs/engine/fsr3-upscaler-integration-plan.md` status header

## Source

Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D23-03).

https://github.com/matiaszanolli/ByroRedux/issues/2824
