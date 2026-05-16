# REN-D10-003: SVGF denoised indirect bound with LINEAR sampler

**GitHub**: #1085  
**Severity**: MEDIUM  
**Domain**: renderer  
**Location**: `crates/renderer/src/vulkan/composite.rs:626-629`

## Root Cause
`indirect_info` (binding 1) uses `hdr_sampler` (LINEAR). The SVGF denoised output is
RGBA16F — LINEAR would work without validation errors but adds unintended spatial blur
to the denoised indirect lighting. The caustic_sampler (NEAREST + CLAMP_TO_EDGE) is 
the correct filter for this binding.

## Fix (1 file)
1. Rename `caustic_sampler` → `nearest_sampler` (dual-purpose: caustic R32_UINT + SVGF indirect)
2. Use `nearest_sampler` for `indirect_info` (binding 1) in new_inner() and recreate_on_resize()
3. Update struct field doc

Files changed: composite.rs (7 replacements for rename + 2 new sites)
