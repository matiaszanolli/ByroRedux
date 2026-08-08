# SKY-D2-01: FO76 BSShaderType155 numbering leaks into the Skyrim-numbered material_kind consumer -- only type 4 is remapped, dropping FO76 hair tint

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2579
**Finding ID**: SKY-D2-01

**Severity**: MEDIUM
**Dimension**: BSLightingShaderProperty / BSEffectShaderProperty Shader-Type Dispatch
**Location**: `crates/nif/src/import/material/shader_data.rs:103-114`; `dedicated_shader.rs:125-206,327`; `byroredux/src/render/static_meshes.rs:480-492`; `crates/renderer/shaders/triangle.frag:1133-1146`
**Status**: NEW

## Description
The parser correctly keeps `parse_shader_type_data` (Skyrim/FO4) and `parse_shader_type_data_fo76` separate, but the importer then writes the raw `BSShaderType155` integer straight into `MaterialInfo.material_kind`, which downstream code consumes as if it were a `BSLightingShaderType`. `apply_shader_type_data` patches exactly one divergent value (type 4 → `material_kind = 5`), leaving three more mismatched: BSShaderType155 3 (Face Tint) vs BSLightingShaderType 3 (Parallax); 5 (Hair Tint) vs 5 (Skin Tint, **unremapped**); 12 (Eye Envmap) vs 12 (Tree Anim); 17 (Terrain) vs 17 (Cloud). The demonstrable loss is type 5: `parse_shader_type_data_fo76` correctly produces `ShaderTypeData::HairTint` and captures `hair_tint_color`, but `material_kind` stays `5`, so the render-data packer's `material_kind == 6` gate never fires — the authored FO76 hair tint is discarded and the mesh renders untinted.

## Evidence
Confirmed directly: `shader_data.rs:112-114` — only `if matches!(data, ShaderTypeData::Fo76SkinTint { .. }) { info.material_kind = 5; }`, no equivalent arm for `HairTint`. Existing test `fo76_skin_tint_remaps_material_kind_to_skyrim_constant` pins only the type-4 case, with no HairTint sibling.

## Impact
FO76 hair meshes lose authored tint uniformly; FO76 FaceTint/EyeEnvmap/Terrain land on the wrong `material_kind` branch or none. **Skyrim SE itself is unaffected** — vanilla Skyrim never produces a `BSShaderType155` value (0 of 81,244 corpus blocks). This is a Skyrim-checklist item ("guard the two enums don't cross-contaminate") whose blast radius lands entirely on FO76.

## Related
#612 (established the incomplete type-4 remap); #2296 (`material_kind` literals not cross-crate pinned)

## Suggested Fix
Translate `BSShaderType155` → canonical `BSLightingShaderType` once at the import boundary, keyed on `scene.bsver >= FO76` (a small `canonical_material_kind(bsver, shader_type)` covering {3→4, 4→5, 5→6, 12→16, 17→17-or-None}), and gate the texture-slot `match` on the same canonical value.

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: New translation function is the single boundary for the FO76→Skyrim numbering conversion, not scattered per-consumer patches
- [ ] **TESTS**: Add a `fo76_hair_tint_remaps_material_kind_to_skyrim_constant` sibling test to the existing skin-tint one
