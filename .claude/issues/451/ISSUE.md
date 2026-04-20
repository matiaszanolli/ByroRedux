# Issue #451

FO3-NIF-M2: BSShaderNoLightingProperty falloff fields captured but dropped by importer

---

## Severity: Medium

**Location**: `crates/nif/src/import/material.rs:653-656`

## Problem

Parser captures all four falloff fields correctly at `crates/nif/src/blocks/shader.rs:102-150` (bsver>26 gated, Oblivion defaults):
- `falloff_start_angle`, `falloff_stop_angle`
- `falloff_start_opacity`, `falloff_stop_opacity`

Importer reads only `file_name` → `info.texture_path`. The four falloff fields are silently dropped because `MaterialInfo` has no NoLighting-falloff fields to receive them (it has them for `BSEffectShaderProperty` at lines 316-320).

## Impact

FO3 UI overlays, VATS crosshair, scope reticles, Pip-Boy glow, heat-shimmer planes — anything using NoLighting with a soft-alpha cone — renders with uniform alpha instead of the angular falloff the author specified. Rare but visually wrong.

## Fix

Mirror the `BSEffectShaderProperty` treatment: extend `MaterialInfo` with the four falloff fields + `soft_falloff_depth` (if NoLighting has it, else just the four), copy values in the `if let Some(shader) = scene.get_as::<BSShaderNoLightingProperty>(idx)` branch at line 653.

## Completeness Checks

- [ ] **TESTS**: Synthetic FO3 NoLighting block → importer produces `MaterialInfo` with falloff values
- [ ] **SIBLING**: Verify `BSEffectShaderProperty` falloff copy at `material.rs:316-320` works end-to-end
- [ ] **RENDER**: Downstream renderer path needs to consume falloff — separate issue if missing

Audit: `docs/audits/AUDIT_FO3_2026-04-19.md` (FO3-NIF-M2)
