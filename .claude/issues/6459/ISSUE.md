# FNV-2026-10-27-02: FNV light-beam and glow VFX STATs get solid synthesized trimesh colliders, because the 10-26 VFX-sheet gate recognises only Skyrim+ BSEffectShaderProperty

Labels: high, physics, game:fnv, legacy-compat, bug
Source: docs/audits/AUDIT_FNV_2026-10-27.md

**Source**: `docs/audits/AUDIT_FNV_2026-10-27.md` finding `FNV-2026-10-27-02` (Dimension 1, cell spawn; mechanism owner `/audit-physics` + the `/audit-nifal` boundary) · Severity HIGH · Status NEW (`1efe39722`, 2026-10-26, fixed the Skyrim dialect only) · Game: FNV (FO3 shares the dialect, not measured). Static evidence, not reproduced in a running engine.

**Location (current)**: `byroredux/src/cell_loader/spawn.rs:76-90` (`render_geometry_can_collide`), `:92-111` (`missing_collision_fallback`: any `RenderLayer::Architecture` base whose NIF has no bhk -> `ArchitectureTriMesh`); `byroredux/src/cell_loader/spawn/mesh_instance.rs:1526-1539` (`spawn_trimesh_collider_ghost`); `crates/plugin/src/record.rs:304-341` (STAT -> Architecture).

## Description
`render_geometry_can_collide` excludes decals, alpha-tested meshes, fire-refraction and `effect_shader.is_some()`. FO3/FNV author their VFX sheets as `BSShaderNoLightingProperty` plus a blended `NiAlphaProperty`. They carry no `effect_shader` and no alpha test, so they pass the predicate. These are god-ray light beams, industrial light rays, glow fills, dust shafts and bright spots. A STAT base sends them through the missing-collision fallback, which builds a Fixed trimesh from the render geometry.

## Evidence
Current predicate:
```rust
fn render_geometry_can_collide(material: &ImportedMaterial) -> bool {
    !material.is_decal
        && !material.alpha_test
        && material.effect_shader.is_none()
        && material.material_kind != byroredux_renderer::MATERIAL_KIND_FIRE_REFRACTION
}
```
Census over `FalloutNV.esm` STAT `MODL` x `Fallout - Meshes.bsa` / `Update.bsa` block types: **98 non-marker STAT bases, 6,763 REFRs (6,726 under `meshes\effects\`)** have no `bhk*` block, an `NiAlphaProperty`, and only `BSShaderNoLightingProperty`. Largest: `FXGlowSimpFillRndFltHalf` x866, `FXLightBeamBrt04` x557, `FXBrightSpotFlat` x556, `FXLightBeamW01Dim` x540, `FXLightBeamW03` x415, `IndFXLightRays01` x298, `FXLightBeamSuperWide01` x277. Six were sampled with `dump_alpha` / `material_dump`: each has `blend=true test=false`, decal 0 and material kind 102 (`MATERIAL_KIND_NO_LIGHTING`), so `render_geometry_can_collide` returns `true`.

## Impact
Light shafts in FNV interiors and lit exteriors become solid. They block the player capsule, NPC KCC walks and dropped bodies, and they intercept physics probes. This is FNV's version of the Riverwood rapids wall (the same mechanism wedged the character capsule per `1efe39722`), spread over thousands of placements.

## Related
`1efe39722`, PHYSAL missing-collision fallback (#2355), FO3 (shared dialect).

## Suggested Fix
Key the gate on a canonical "VFX / does not collide" signal set at the NIFAL boundary, not on one game's shader block. For example, exclude `MATERIAL_KIND_NO_LIGHTING` (or blended + no-depth-write unlit). Then add an FNV real-data pin that the census bases synthesize no collider.

## Completeness Checks
- [ ] **SIBLING**: FO3 and Oblivion VFX dialects, and the PackedAabbProxy arm of `missing_collision_fallback`, checked for the same pass-through
- [ ] **CANONICAL-BOUNDARY**: The "does not collide" signal is set at the NIFAL parser->`Material` boundary (`translate_material` / import), never derived from a per-game shader block at spawn or in the renderer
- [ ] **TESTS**: An FNV real-data pin asserts the census bases (e.g. `FXLightBeamBrt04`) spawn no trimesh collider

