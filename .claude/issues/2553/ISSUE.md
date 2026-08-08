# FNV-D2-01: Canonical Material.specular_color carries FNV's universally-black NiMaterialProperty.specular, zeroing the entire direct-specular BRDF term

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2553
**Finding ID**: FNV-D2-01

**Severity**: HIGH
**Dimension**: NIFAL Canonical Translation (FNV slice)
**Location**: `crates/nif/src/import/material/legacy_properties.rs:142`, `byroredux/src/material_translate.rs:124`, `crates/renderer/shaders/include/lighting.glsl:168`
**Status**: NEW

## Description
On FNV (BSVER 34), `NiMaterialProperty` serializes only Specular + Emissive (no Ambient/Diffuse). Vanilla FNV authors Specular = (0,0,0) essentially everywhere, because the source engine's lit pipeline (`BSShaderPPLightingProperty`) sources specular from the shader/normal-map alpha, not the material property. `apply_material_property` faithfully copies that authored zero through the NIFAL boundary into the canonical `Material.specular_color`, and the shader's BRDF multiplies the entire GGX lobe by it with no floor.

## Evidence
Byte-level trace (`metalbox01.nif`) confirms the parse is correct and the data really is black; `material_dump` across 6 vanilla NIF files / 18 meshes shows `specClum = 0.00` on every single one (with `specS = 1.00`, ruling out the disabled-specular path). Worst on keyword-classified conductors — `MetalBox01:0` (metalness 0.90) collapses both diffuse (~10%) and specular (0) simultaneously, rendering near-black under direct light. Confirmed directly: `legacy_properties.rs:142` does `info.specular_color = [mat.specular.r, mat.specular.g, mat.specular.b];` with no zero-guard, and `material_translate.rs:124` forwards it verbatim (`specular_color: source.specular_color`).

## Impact
Zeros the entire direct-specular BRDF term on every FNV surface, worst on keyword-classified metals (near-black under direct light). This is the reference title's most fundamental material behavior — a systemic, silently-live rendering divergence, not a narrow edge case.

## Related
#696, #1873 (`specular_authored`), FNV-D2-02 (this report — same zeros feeding the PBR classifier's dead metalness-lift arm).

## Suggested Fix
Resolve at the NIFAL boundary (`legacy_properties.rs`), not the shader: treat an all-zero `NiMaterialProperty.specular` co-bound with `BSShaderPPLightingProperty`/`BSShaderNoLightingProperty` as *unauthored* rather than *authored-off* — leave `specular_color` neutral `[1,1,1]` with `specular_authored = false`. Keep the explicit `NiSpecularProperty{flags:0}` → zero path intact (that IS an authored disable). Verify the neutral-value choice against Gamebryo 2.3 source per the no-guessing policy before implementing.

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: The unauthored-vs-authored-off distinction is made once at the NIFAL boundary, never re-derived at render time
- [ ] **TESTS**: A regression test confirms an all-zero FNV `NiMaterialProperty.specular` produces `specular_color = [1,1,1]`, `specular_authored = false`, while an explicit `NiSpecularProperty{flags:0}` still zeros it
- [ ] **SIBLING**: Verify Gamebryo 2.3 source before implementing (per the no-guessing policy)
