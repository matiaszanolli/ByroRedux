# Issue #1021

**Title**: REN-D18-002: Volumetric HG asymmetry g not clamped — g≈±1 produces NaN

**Source**: `docs/audits/AUDIT_RENDERER_2026-05-13.md` — REN-D18-002
**Severity**: MEDIUM (defensive — current host default 0.4 is safe but channel is exposed)
**File**: `crates/renderer/shaders/volumetric_inject.comp` (HG phase function)

## Issue

Henyey-Greenstein anisotropy parameter `g` is exposed via params UBO but not clamped in-shader. `g = ±1` produces div-by-zero in the standard HG phase: `(1 - g²) / (1 + g² - 2g·cos(θ))^1.5`. Current host wires `DEFAULT_PHASE_G = 0.4` so it's safe today, but the UBO field is hot for runtime tuning (console command, weather record).

## Fix

Shader-side clamp: `float g_safe = clamp(g, -0.999, 0.999);` before the HG denominator.

## Completeness Checks
- [ ] **UNSAFE**: N/A — GLSL
- [ ] **SIBLING**: Check any other phase function in caustic_splat.comp or composite for same pattern
- [ ] **TESTS**: NaN-injection test with g=1.0

