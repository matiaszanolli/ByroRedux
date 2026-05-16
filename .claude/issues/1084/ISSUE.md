# REN-D18-002: Interior cells darken by 63% due to volumetric extinction

**GitHub**: #1084  
**Severity**: MEDIUM  
**Domain**: renderer  
**Location**: `crates/renderer/src/vulkan/context/draw.rs:2176`

## Root Cause
`camera_pos.w = DEFAULT_SCATTERING_COEF (0.005)` is sent regardless of interior/exterior.
Interior cells zero `sun_color` (no inscatter) but scattering drives extinction too.
T_cum = exp(-0.005 × 200m) ≈ 0.37 → composite multiplies scene × 0.37 + 0 → 63% dark.

## Fix (1 file, 1 line)
Zero scattering_coef when `!sky_params.is_exterior`:
`if sky_params.is_exterior { DEFAULT_SCATTERING_COEF } else { 0.0 }`

This makes T_cum = exp(0) = 1.0 → composite is scene × 1 + 0 = scene (no-op).
