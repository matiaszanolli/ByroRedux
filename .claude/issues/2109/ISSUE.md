# SF-D9-02: BGEM v21/v22 glass-overlay params + envmap-mask-scale + v11 emittance dropped in merge

**Severity**: LOW
**Labels**: low, renderer, enhancement
**Location**: `byroredux/src/asset_provider/material.rs:973-1102`; fields at `crates/bgsm/src/bgem.rs:31-77`
**Source audit**: `docs/audits/AUDIT_STARFIELD_2026-07-16.md` (SF-D9-02)

## Description
The BGEM merge arm forwards `glass_enabled` but drops `glass_fresnel_color`, `glass_refraction_scale_base`, `glass_blur_scale_base`, `glass_blur_scale_factor`, `glass_roughness_scratch`, `glass_dirt_overlay` (all FO76/Starfield-era), plus `environment_mapping_mask_scale` and `emittance_color` (the latter already explicitly deferred in-code). No `ImportedMesh` sink exists for any of these.

## Impact
Mod-added Starfield/FO76 BGEM glass renders with engine-default refraction/tint instead of authored values. Low severity: the renderer currently has no binding to consume these fields even if forwarded, so this is a deferred-consumer gap, not an active miswrite.

## Suggested Fix
Track as a deferred renderer-binding follow-up (glass refraction/fresnel/blur), paired with the already-noted `emittance_color` second-emissive-slot deferral. No parser change needed.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
