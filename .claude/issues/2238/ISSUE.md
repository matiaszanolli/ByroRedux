# REN-D14-01: MultiLayerParallax is a caustic source per the CPU gate but never enters SHADOW_MASK_GLASS per the TLAS mask assignment

Severity: medium
Source audit: docs/audits/AUDIT_RENDERER_2026-08-02.md
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2238

**Dimension**: 14 (Caustics / TLAS masks)
**Location**: `crates/renderer/shaders/caustic_splat.comp:13` (`is_caustic_source` comment: "MATERIAL_KIND_GLASS or MultiLayerParallax"); `crates/renderer/src/vulkan/acceleration/predicates.rs:594-614` (`shadow_mask_for_instance` — only `MATERIAL_KIND_GLASS` gets `SHADOW_MASK_GLASS`; `MultiLayerParallax` — `material_kind == 11` — falls into the `SHADOW_MASK_OPAQUE` else-branch)
**Status**: NEW

**Description**: `MultiLayerParallax` (`material_kind == 11`, per `byroredux/src/render/static_meshes.rs:472`) is treated as a caustic-refraction *source* by the CPU-side caustic gate (matching `MATERIAL_KIND_GLASS`), but `shadow_mask_for_instance` only assigns `SHADOW_MASK_GLASS` to literal `MATERIAL_KIND_GLASS` (100) — MLP instances get the default `SHADOW_MASK_OPAQUE` path instead. A refractor that's a caustic source but shadow-masked as opaque can receive its own caustic on its own back face.

**Impact**: MultiLayerParallax refractors can self-illuminate their own back face with a caustic they themselves cast, a visible artifact on any MLP-shaded surface (ice, some water-adjacent Skyrim+ materials).

**Suggested Fix**: add the MultiLayerParallax material kind to the `SHADOW_MASK_GLASS` branch in `shadow_mask_for_instance` alongside `MATERIAL_KIND_GLASS`.

## Completeness Checks
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
