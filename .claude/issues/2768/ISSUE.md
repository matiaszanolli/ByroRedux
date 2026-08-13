# REN-D13-03: taa.rs dispatch hardcodes div_ceil(8) instead of generated workgroup constants

## Description
Hard-codes `div_ceil(8)` instead of the generated `WORKGROUP_X`/`WORKGROUP_Y` that `taa.comp`'s `local_size` is built from. Lowering the tile would leave the bottom-right of the TAA output **never written** (that slot's history retains the previous cycle's contents, which composite then samples as this frame's HDR). `bloom.rs` and `volumetrics.rs` already import the constants; `taa.rs`, `svgf.rs`, `ssao.rs` and `caustic.rs` still use the literal.

## Location
`crates/renderer/src/vulkan/taa.rs` (`TaaPipeline::dispatch`)

## Severity / Domain / Type
low / renderer / bug

https://github.com/matiaszanolli/ByroRedux/issues/2768

Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D13-03).
