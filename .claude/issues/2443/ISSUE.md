# MAT-D3-01: grayscale_to_palette_scale dead-ends at the NIFAL boundary — captured by both importers, no canonical Material field to land in

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2443
**Finding ID**: MAT-D3-01 (source: `docs/audits/AUDIT_LEGACY_COMPAT_2026-08-07.md`)

**Severity**: MEDIUM
**Dimension**: 3 — Material translation boundary (NIFAL reference slice)
**Location**: `byroredux/src/material_translate.rs:120-215` (no copy); `crates/core/src/ecs/components/material.rs:55-304` (no field); producers `crates/nif/src/import/types.rs:489`, `byroredux/src/asset_provider/material.rs:1065-1067`, `crates/nif/src/blocks/shader.rs:724`
**Status**: NEW (same shape as fixed #2284, distinct field)

## Description
`ImportedMaterial.grayscale_to_palette_scale` is populated from both the inline NIF shader block (BSVER≥130 `BSLightingShaderProperty`/`BSEffectShaderProperty`) and the BGSM/BGEM merge (with parent-template precedence + a dedicated round-trip test), but `translate_material` never copies it — the canonical `Material` has no such field. `triangle.frag:984-987` documents this explicitly: "not yet plumbed to GpuMaterial — direct lookup for now." #2284's landing comment names this exact field as the precedent that justified its own fix — this is the one remaining instance of that pattern.

## Impact
FO4/FO76/Starfield content authoring a sub-1.0 palette scale (de-saturating a shared greyscale ramp) renders the palette remap at full strength; because `EFFECT_PALETTE_COLOR`/`ALPHA` is a replace not a blend, an authored 0.5 scale that should soften the remap produces the full palette colour instead.

## Related
#2284 (CLOSED, MAT-D1-NEW-04 — the sibling six-field fix that named this field as its own precedent).

## Suggested Fix
Add `grayscale_to_palette_scale: f32` (default 1.0) to `Material` and copy it in `translate_material` (closes the silent drop); plumb to `GpuMaterial`/shader as a separate follow-up.

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: New field copied only inside `translate_material`, never re-derived at render time
- [ ] **TESTS**: A regression test confirms the field survives from `ImportedMaterial` through `translate_material` into `Material`
