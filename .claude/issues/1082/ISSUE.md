# REN-D18-001: Froxel images not cleared after layout transition

**GitHub**: #1082  
**Severity**: HIGH  
**Domain**: renderer  
**Location**: `crates/renderer/src/vulkan/volumetrics.rs`

## Root Cause
`initialize_layouts()` transitions froxel images UNDEFINED → GENERAL but never calls
`cmd_clear_color_image`. Module doc says "clears them to (0 scattering, 1 transmittance)" — false.
Composite formula: `combined = combined * vol.a + vol.rgb`; undefined `vol.a ≈ 0` collapses scene.

## Fix
1. Add `TRANSFER_DST` to froxel image usage flags in `allocate_froxel_slot()` (required by `cmd_clear_color_image`)
2. After layout transition in `initialize_layouts()`: clear each image to (0,0,0,1), then barrier TRANSFER → COMPUTE
3. Fix doc: remove false "compute pass that clears" claim; update to say initialize_layouts clears

## Files Changed
- `crates/renderer/src/vulkan/volumetrics.rs` (1 file)
