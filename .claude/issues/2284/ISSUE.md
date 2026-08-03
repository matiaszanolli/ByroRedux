# MAT-D1-NEW-04: six authored Skyrim+/FO4 BSLightingShaderProperty shading scalars captured at import, silently dropped at the canonical Material boundary

Source: `docs/audits/AUDIT_NIFAL_2026-08-03.md`

**Severity**: MEDIUM
**Dimension**: Material · **Tier Violated**: no-leak
**Game Affected**: Skyrim LE/SE (BSVER 83–129: `lighting_effect_1/2`), FO4/FO76/Starfield (BSVER 130+: `subsurface_rolloff`, `rimlight_power`, `backlight_power`, `fresnel_power`)
**Location**: captured at `crates/nif/src/import/types.rs:484-491`, copied at `crates/nif/src/import/material/mod.rs:1239-1246`, also independently sourced from BGSM at `byroredux/src/asset_provider/material.rs:1029`; never read by `byroredux/src/material_translate.rs` (no field exists on `crates/core/src/ecs/components/material.rs`'s canonical `Material`, nor on `GpuMaterial`)

## Description

`#1241`'s own regression-test doc comment states the goal explicitly: 8
`BSLightingShaderProperty` PBR scalars must land on `MaterialInfo` and
propagate through every mesh extractor into `ImportedMesh`. That half (raw
tier) is done and tested. Of the 8, `refraction_strength` and
`grayscale_to_palette_scale` (explicitly documented as deferred at
`triangle.frag:984`) complete the translate step. The other 6 —
`lighting_effect_1`, `lighting_effect_2`, `subsurface_rolloff`,
`rimlight_power`, `backlight_power`, `fresnel_power` — dead-end on
`ImportedMaterial` with zero consumers anywhere in `byroredux/src/`,
`crates/debug-server/`, or `crates/debug-protocol/` (verified by repo-wide grep).

## Evidence

`grep -rn "lighting_effect_1\|subsurface_rolloff\|rimlight_power\|backlight_power" byroredux/src crates/debug-server crates/debug-protocol` returns nothing; `translate_material`'s `Material { ... }` literal has no corresponding fields.

## Impact

Skin/hair/cloth materials on Skyrim LE/SE and FO4/FO76/Starfield that author
non-default rim-lighting, backlight, subsurface-rolloff, or Fresnel-exponent
values (a routine Bethesda skin-shader authoring pattern) render with the
engine's fixed Disney BSDF response instead of the author's tuned curve.
Shading-fidelity gap only (nothing crashes or renders as the wrong *kind*), so
it stays below HIGH — but it is genuine authored-data loss, and `nifal.md`'s
"Materials — converged" verdict slightly overstates completeness.

## Suggested Fix

Add the missing fields to the canonical `Material` (mirroring how
`translucency_*` was added in `#1147`) and to `GpuMaterial`, copy them in
`translate_material`, and wire a `triangle.frag` consumer — or at minimum land
them on `Material` and note in `nifal.md` as "captured, not yet shaded,"
matching the existing `grayscale_to_palette_scale` precedent.

## Filed as

GitHub issue #2284, labels: `medium`, `nif-parser`, `renderer`, `bug`.
