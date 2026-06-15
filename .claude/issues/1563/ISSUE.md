# documentation, low, pipeline

## REN-D11-02: shader-pipeline.md lists HDR attachment as B10G11R11_UFLOAT_PACK32; live format is R16G16B16A16_SFLOAT (alpha load-bearing)

**Severity**: LOW
**Dimension**: Pipeline/RenderPass
**Source audit**: `docs/audits/AUDIT_RENDERER_2026-06-14.md`
**Status**: NEW

## Description
`docs/engine/shader-pipeline.md` claims the HDR colour attachment is `B10G11R11_UFLOAT_PACK32` (no alpha). The actual format (`GBufferFormats.color_format = HDR_FORMAT`) is `R16G16B16A16_SFLOAT`; the blend + water pipelines depend on that alpha channel for SRC_ALPHA blending.

## Evidence
- `crates/renderer/src/vulkan/composite.rs:126` — `pub const HDR_FORMAT: vk::Format = vk::Format::R16G16B16A16_SFLOAT;`
- `crates/renderer/src/vulkan/context/helpers.rs` inline comment "0 — HDR color (RGBA16F)".
- Shaders write `outColor = vec4(..., alpha)`; blend factors `src_alpha` / `one_minus_src_alpha`.
- `docs/engine/shader-pipeline.md:91` — HDR colour row says `B10G11R11_UFLOAT_PACK32`.

## Impact
Doc-only but actively misleading: a contributor "fixing" the attachment to the documented packed (alpha-less) format would silently break alpha-blended / water output.

## Suggested Fix
Correct the HDR row to `R16G16B16A16_SFLOAT` and note the alpha feeds blend/water.

## Completeness Checks
- [ ] **SIBLING**: confirm the doc's other format rows (raw indirect, albedo at `B10G11R11`) actually match code before trusting them
