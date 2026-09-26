# REN-D5-2026-09-26-09: `parse_dds` never validates that cubemap faces are square before `CUBE_COMPATIBLE` image creation

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/4887

**Labels**: medium,renderer,safety,bug

- **Severity**: MEDIUM (untrusted-input reader passes an invalid image description to `vkCreateImage`; defence in depth)
- **Dimension**: Memory/Lifecycle
- **Location**: `crates/renderer/src/vulkan/dds.rs` — `parse_dds` (legacy `DDSCAPS2_CUBEMAP` path and the DX10 `D3D10_RESOURCE_MISC_TEXTURECUBE` path); consumer `crates/renderer/src/vulkan/texture.rs` (`record_dds_upload`, `ImageCreateFlags::CUBE_COMPATIBLE`, six array layers)
- **Status**: NEW
- **Description / Evidence**: The #4511 caps check `0 < w,h <= 8192` and clamp `mip_count`. A grep of `dds.rs` finds the six-face caps check and the cubemap flags but no `width == height` check. Vulkan's valid-usage rules for `CUBE_COMPATIBLE` images require equal width and height. Archive-supplied DDS (BSA, and BA2 via `build_dds_header`, which passes u16 dimensions straight through) can be mod-authored.
- **Impact**: Likely a driver error routed through the finding-02 path, but the behaviour is undefined rather than guaranteed.
- **Suggested Fix**: `ensure!(width == height)` when `legacy_cubemap || is_cubemap`, with a test on the existing `legacy_cubemap_exposes_six_faces` / `dx10_cubemap_exposes_six_faces` fixtures using unequal dimensions.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (sibling constructors / upload paths / docs)
- [ ] **TESTS**: A regression test pins this specific fix
