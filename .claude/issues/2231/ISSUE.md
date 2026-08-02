# REN-D5-03: Boot-generated volumetric density noise is regenerated on the CPU on every window resize

Severity: medium
Source audit: docs/audits/AUDIT_RENDERER_2026-08-02.md
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2231

**Dimension**: 5 (Memory)
**Location**: `crates/renderer/src/vulkan/volumetrics.rs` (`VolumetricsPipeline::initialize_layouts`, line 1190; calls `generate_density_noise` at lines 1197-1198); `crates/renderer/src/vulkan/context/resize.rs:808` (`new_volumetrics.initialize_layouts(...)`, invoked from the swapchain-recreate path)
**Status**: NEW

**Description**: `initialize_layouts`'s own doc comment says "Call once after `new()`", but `resize.rs` constructs a new `VolumetricsPipeline` and calls `initialize_layouts` on every window resize, which regenerates both the base and detail procedural density-noise volumes from scratch on the CPU — a real, if bounded, ~10^7 hash-evaluation stall on every resize.

**Evidence**: `volumetrics.rs:1197-1198` calls `generate_density_noise(BASE_NOISE_SIZE, false)` / `generate_density_noise(DETAIL_NOISE_SIZE, true)` unconditionally inside `initialize_layouts`; `resize.rs:808` calls this on the resize/recreate path, contradicting the "call once" doc comment.

**Impact**: A user resizing the window (or a resolution-scaling event, given the grid is now resolution-scaled per REN-D5-01) pays a CPU hitch proportional to noise-volume voxel count every time, instead of only at startup.

**Suggested Fix**: cache the generated noise texels (or the noise itself, since it's resolution-independent) across resize and only re-upload/rebind rather than regenerating.

## Completeness Checks
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
