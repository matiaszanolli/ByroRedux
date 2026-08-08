# REN-D11-2026-08-07-05: G-buffer colour formats are never format-feature-queried, unlike depth

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2502
**Finding ID**: REN-D11-2026-08-07-05 (source: `docs/audits/AUDIT_RENDERER_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 11 — Pipeline/RenderPass
**Location**: `crates/renderer/src/vulkan/gbuffer.rs:39-72` (format consts) + `crates/renderer/src/vulkan/context/helpers.rs:22` (`find_depth_format` — the only `get_physical_device_format_properties` call in the crate)
**Status**: NEW

## Description
The depth format is chosen by querying `optimal_tiling_features` for `DEPTH_STENCIL_ATTACHMENT`. Every colour attachment format is a hard-coded const with no capability query and no fallback. Most are fine — `R16G16_SFLOAT`, `R32_UINT`, `R8_UNORM`, `B10G11R11_UFLOAT_PACK32` and `R16G16B16A16_SFLOAT` all carry mandatory `COLOR_ATTACHMENT` (and, where blended, `COLOR_ATTACHMENT_BLEND`) in the Vulkan mandatory-format table. The exception is `NORMAL_FORMAT = R16G16_SNORM`: 16-bit SNORM formats are mandatory only for `SAMPLED_IMAGE` / `SAMPLED_IMAGE_FILTER_LINEAR` / `BLIT_SRC` / `VERTEX_BUFFER`, **not** for `COLOR_ATTACHMENT`.

## Evidence
`grep -rn "get_physical_device_format_properties" crates/renderer/src/` returns exactly one hit (`helpers.rs:33`, inside `find_depth_format`). `gbuffer.rs::Attachment::allocate` creates the normal image with `COLOR_ATTACHMENT | SAMPLED` unconditionally.

## Impact
On a conformant device that does not expose `COLOR_ATTACHMENT` for `R16G16_SNORM`, `create_image` fails with `VK_ERROR_FORMAT_NOT_SUPPORTED` during `GBuffer::new` and the engine refuses to start with a generic "Failed to create gb_normal image". Loud, not silent — and no desktop driver in the target hardware class (RTX 4070 Ti dev GPU, and AMD/Intel desktop) actually lacks it. This is a portability / diagnostics gap, not a live defect.

## Related
#275 (introduced octahedral RG16_SNORM normals); REN-D4-NEW-02 (`AUDIT_RENDERER_2026-05-11_DIM4.md`) applied the same "query before you commit to a format" reasoning to depth only.

## Suggested Fix
Add a one-shot startup check that asserts `COLOR_ATTACHMENT` in `optimal_tiling_features` for each G-buffer colour format (plus `COLOR_ATTACHMENT_BLEND` for the four blended by the blend/water pipelines), failing with a format-naming error. A real fallback format for normals is not worth it; a precise error message is.

## Completeness Checks
- [ ] **TESTS**: A startup format-capability check is added and produces a named error on failure (manual verification — no non-conformant device in dev fleet)
