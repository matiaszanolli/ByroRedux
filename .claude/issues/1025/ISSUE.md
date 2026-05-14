# Issue #1025

**Title**: F-WAT-04: Water grazing-angle normal-clamp mixes only 60% — can still go below the plane

**Source**: `docs/audits/AUDIT_RENDERER_2026-05-13.md` — F-WAT-04
**Severity**: MEDIUM
**File**: `crates/renderer/shaders/water.frag:341-343`

## Issue

`WaterPipeline` cull mode is static NONE but the shader's reflection-ray normal-clamp at lines 341-343 mixes only 60% toward geometric N when `dot < 0.05`. Result: at very grazing angles the perturbed normal can still tilt below the water plane, producing reflection rays that hit the water from underneath.

## Fix

Mix 100% (or use raw geometric N) when the perturbed normal would produce a sub-plane reflection vector. Equivalent: clamp `Nperturbed.y` to `>= geometric.y * 0.999` after perturbation.

## Completeness Checks
- [ ] **SIBLING**: Same clamp policy on refraction normal (line 365)?
- [ ] **TESTS**: Grazing-angle camera looking along water surface should produce no sub-plane reflection rays

