# SUBSYS-02: NiVertexColorProperty is suppressed by NiMaterialProperty on the legacy property chain (re-introduces the #435/N06 bug class)

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2457
**Finding ID**: SUBSYS-02 (source: `docs/audits/AUDIT_LEGACY_COMPAT_2026-08-07.md`)

**Severity**: MEDIUM
**Dimension**: 7 — Subsystem coverage vs legacy
**Location**: `crates/nif/src/import/material/legacy_properties.rs:717-748` (gate at `:742`), interacts with `:133-154`
**Status**: NEW

## Description
`apply_vertex_color_property` writes `vertex_color_mode` only `if !info.has_material_data`. The intent (#1208) was Skyrim-specific (stop an inherited `NiVertexColorProperty` overriding a `BSLightingShaderProperty` default), but `has_material_data` is *also* set by the ordinary pre-Skyrim `NiMaterialProperty` arm. Since the property chain walks in file order, a `NiMaterialProperty` visited before a `NiVertexColorProperty` (the common Oblivion/FO3/FNV property order) latches the gate shut, dropping every later vertex-color property regardless of direct-vs-inherited status. This is the identical failure the codebase already diagnosed and fixed once for the sibling UV-transform flag (#435/N06) — the fix there was to split out a narrow `has_uv_transform` flag; that lesson wasn't applied here.

## Evidence
Trace for `[NiMaterialProperty, NiVertexColorProperty]`: iteration 1 sets `has_material_data=true`; iteration 2 reaches the gate false and skips the write. The regression suite covers only BSL+NVCP and no-shader-property+NVCP — never `NiMaterialProperty`+NVCP, the dominant legacy shape.

## Impact
Order-dependent, silent. A pre-Skyrim shape authoring `SOURCE_IGNORE` gets vertex colours applied anyway (over-darkened Oblivion/FO3 architecture/clutter carrying baked-AO vertex colour the property explicitly disabled). A shape authoring `SOURCE_EMISSIVE` alongside `NiMaterialProperty` loses emissive routing, falling back to albedo modulation — the class of bug #695 fixed at the shader end (torches/glowing signs going flat-lit).

## Related
#435/N06 (CLOSED — identical failure shape, previously fixed for UV transform), #695 (CLOSED — the shader-end fix for the emissive-routing symptom this reintroduces upstream).

## Suggested Fix
Mirror the #435 remedy — add a dedicated `vertex_color_mode_consumed` flag set only by the sites that genuinely author vertex-colour intent, and gate on that instead of `has_material_data`. Extend the precedence test suite with a `NiMaterialProperty`-before-NVCP case.

## Completeness Checks
- [ ] **TESTS**: New precedence test covers `NiMaterialProperty`-before-`NiVertexColorProperty` (the dominant legacy shape, currently untested)
- [ ] **SIBLING**: Confirm no other `has_material_data`-gated site has the same over-broad-gate shape
