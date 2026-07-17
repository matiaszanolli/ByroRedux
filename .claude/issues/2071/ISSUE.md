# TD2-112: GENERAL to GENERAL compute-write-to-shader-read ImageMemoryBarrier hand-rolled 7x across compute passes

**GitHub Issue**: #2071
**Labels**: low,vulkan,tech-debt,bug

**Severity**: LOW
**Dimension**: 2 (Logic Duplication)
**Location**: `svgf.rs:1227,1304`, `taa.rs:784`, `caustic.rs:908`, `volumetrics.rs:927,977`, `water_caustic.rs:371`

## Description
`descriptors.rs` has a helper for the UNDEFINED→GENERAL init shape (`image_barrier_undef_to_general`) but nothing for the GENERAL→GENERAL compute-write-to-shader-read shape; all 7 sites build the byte-identical `ImageMemoryBarrier` struct by hand.

## Evidence
Confirmed live: `svgf.rs:1227`, `taa.rs` (`~784`), and `caustic.rs` (`~908`) all construct `vk::ImageMemoryBarrier::default().src_access_mask(SHADER_WRITE).dst_access_mask(SHADER_READ).old_layout(GENERAL).new_layout(GENERAL)...` verbatim. `crates/renderer/src/vulkan/descriptors.rs` defines `image_barrier_undef_to_general`, `image_barrier_undef_to_transfer_dst`, `image_barrier_transfer_dst_to_shader_read` but no GENERAL→GENERAL write-to-read variant.

## Related
#1751/TD2-002, #1752/TD2-003-004 (closed, fixed adjacent duplication, didn't cover this barrier shape).

## Suggested Fix
Add `image_barrier_general_write_to_read(image)` to `descriptors.rs`, swap all 7 sites; stage masks stay caller-owned (they legitimately vary).

**Effort**: small

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — all 5 files (svgf/taa/caustic/volumetrics/water_caustic) share this exact barrier shape at their respective compute→composite handoff points
- [ ] **TESTS**: No unit test practical for a raw Vulkan barrier — rely on existing RT/compute-pass smoke tests (svgf/taa/caustic golden-frame tests) to catch any regression
