# Issue #1022

**Title**: REN-D18-008: Volumetric inject sun direction hardcoded — interior cells get sun-shaft injection

**Source**: `docs/audits/AUDIT_RENDERER_2026-05-13.md` — REN-D18-008
**Severity**: MEDIUM (latent — fires the moment volumetrics is re-enabled per #928)
**File**: `crates/renderer/src/vulkan/context/draw.rs:2116`

## Issue

`sun_dir` for the volumetric inject pass is hardcoded `[-0.4, 0.8, -0.45]` instead of being plumbed from `SkyParamsRes.sun_direction`. Interior cells (no exterior sun) will inject daylight god-rays through walls when volumetrics is re-enabled.

## Fix

Plumb `SkyParamsRes.sun_direction` and zero `sun_color` when no exterior sun (interior cells, sun below horizon). Fix in lockstep with #928 gate flip and REN-D18-001 vol.a multiply.

## Completeness Checks
- [ ] **SIBLING**: Coordinate with REN-D15-NEW-08 (sun arc fix) and REN-D18-001 (composite vol.a multiply)
- [ ] **TESTS**: Interior-cell volumetric output should be neutral (no god-rays through walls)

