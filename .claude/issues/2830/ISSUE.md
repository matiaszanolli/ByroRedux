# REN-D23-06: FrameExtentSet::for_output's render-extent clamp is dead and would corrupt jitter math if live

Labels: low, renderer, bug

## Description

The `.min(max_image_dimension_2d)` clamp on the SDK-queried **render** extent is dead (the function already rejects an over-limit `output` before the query, and every preset returns render ≤ output). Worse if it ever became live: it would rewrite the render extent *after* the SDK produced it, so `FsrTemporalState::new`'s `jitter_phase_count` and the `render_size` handed to `dispatch` would describe a ratio the SDK never sanctioned — precisely the hand-computed-vs-queried mismatch the module's own doc header exists to prevent. `every_fsr_preset_uses_the_sdk_resolution_query` asserts the unclamped values only. A trap for whoever next raises the output ceiling or adds a preset.

## Location

`crates/renderer/src/vulkan/upscaling.rs` (`FrameExtentSet::for_output`)

## Source

Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D23-06).

https://github.com/matiaszanolli/ByroRedux/issues/2830
