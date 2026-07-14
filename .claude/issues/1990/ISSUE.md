**Severity**: low
**Dimension**: Denoiser/Composite (renderer audit 2026-07-14, DIM8)
**Location**: `crates/renderer/src/vulkan/svgf.rs` module docstring "Descriptor set (binding layout)" table, binding-6 (`outIndirect`) row (line ~42)
**Status**: NEW (CONFIRMED against HEAD)

## Description
The `svgf.rs` module-doc binding table lists binding 6 (`outIndirect`) as `rgba16f`. The shader declares it `r11f_g11f_b10f` and the backing image uses `INDIRECT_HIST_FORMAT = B10G11R11_UFLOAT_PACK32`. Only binding 7 (`outMoments`, `MOMENTS_HIST_FORMAT = R16G16B16A16_SFLOAT`) is actually `rgba16f`. The `e4d574dc` docstring sweep (#1894/#1895) corrected other SVGF docstring facts but missed this row.

## Evidence
- `svgf.rs:42` — `//! | 6 | out indirect | image2D (rgba16f, storage) |` (stale)
- `svgf_temporal.comp:40` — `layout(set = 0, binding = 6, r11f_g11f_b10f) uniform writeonly image2D outIndirect;`
- `svgf.rs` — `const INDIRECT_HIST_FORMAT: vk::Format = vk::Format::B10G11R11_UFLOAT_PACK32;` feeds binding 6.

## Impact
None at runtime. Could mislead a maintainer into widening the indirect history image to 8 B/px, undoing the memory the #275 note deliberately halved.

## Suggested Fix
Change the binding-6 cell to `image2D (r11f_g11f_b10f / B10G11R11, storage)`; leave binding 7 as `rgba16f`.

## Completeness Checks
- [ ] **SIBLING**: Verify the composite / à-trous descriptor docstrings name the same `B10G11R11` format for the indirect history image.
- [ ] **TESTS**: N/A (doc-only).
