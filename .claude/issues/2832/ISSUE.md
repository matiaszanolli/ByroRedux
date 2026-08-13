# REN-D23-07: frame_upscaler.rs 'frame parameters absent' log::error! string is malformed

Labels: low, renderer, bug

## Description

The `log::error!` string contains ~18 literal spaces mid-sentence — a multi-line literal that lost its `\` continuation. This is the **only** signal for a degradation that latches FSR off for the rest of the swapchain generation, and `docs/engine/fsr3-troubleshooting.md` tells operators to grep for exactly these phrases, so a grep on the wrapped phrase misses it. The sibling message directly below uses the correct form.

## Location

`crates/renderer/src/vulkan/frame_upscaler.rs` (the `frame parameters absent` arm of `record`)

## Source

Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D23-07).

https://github.com/matiaszanolli/ByroRedux/issues/2832
