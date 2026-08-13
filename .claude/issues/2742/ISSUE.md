# REN-D6-2026-08-12-01: SkinTint/HairTint arm intercepts the slot-7 specular rule

**Severity**: HIGH
**Dimension**: 6 — NIFAL Material
**Location**: `crates/nif/src/import/material/dedicated_shader.rs` — the `5 | 6 =>` arm vs. the `_ =>` default arm's slot-7 read. Sink: `MaterialInfo::specular_map` → `GpuMaterial::specular_map_index` → `crates/renderer/shaders/triangle.frag`.

## Description

The model-space-normals slot-7 specular rule is implemented once, in the `_ =>` default arm. `BSLightingShaderType` 5 (SkinTint) / 6 (HairTint) are diverted to their own arm (added under #1350) that reads no slot ≥ 3 at all. Every model-space-normal SkinTint material therefore loses its specular map.

## Evidence

Measured on real data: 390/390 slot-7-bearing SkinTint properties in `Skyrim - Meshes0.bsa` (plus 4/4 in `Meshes1.bsa`) are model-space-normal — 100% overlap with the population the rule exists for. No fallback masks it (Skyrim `_msn` maps are DXT1, which `format_has_alpha` excludes).

## Impact

100% of Skyrim SE body/hands/beast-skin specular masks silently dropped — the most common NPC material population in the game. Third member of the #2693/#2694 shader-type-arm-interception family.

## Suggested Fix

Hoist the slot-7 MSN specular read out of the `_ =>` arm so it also applies to `5 | 6 =>`.

## Related

#2693, #2694 (same family, both fixed same day)

Filed from `docs/audits/AUDIT_RENDERER_2026-08-12b.md` (finding REN-D6-2026-08-12-01).
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2742
