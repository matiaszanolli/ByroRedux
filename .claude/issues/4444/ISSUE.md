# #4444: REN-2026-09-16-D6-02: The parallax and IOR neutral defaults are still restated as literals at five production sites after #3073/#3912 named them

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4444
- **Labels**: low,renderer,nifal,tech-debt,bug
- **Filed**: 2026-09-16 via /audit-publish

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-16.md` (texture-roles-deep audit suite, HEAD `7996edf61`)

- **Severity**: LOW (the values currently agree)
- **Dimension**: NIFAL Material / Material Table
- **Location**:
  - `GpuMaterial::default` (`crates/renderer/src/vulkan/material.rs`):
    `parallax_height_scale: 0.04`, `parallax_max_passes: 4.0`. It already
    imports `DEFAULT_DIELECTRIC_IOR` from the same module.
  - `emit_particles` (`byroredux/src/render/particles.rs`): `0.04`, `4.0` and
    `ior: 1.5`. The same literal block already uses
    `DEFAULT_GLASS_REFRACTION_SCALE` / `DEFAULT_GLASS_BLUR_SCALE` (#3912).
  - The `MaterialTextureHandles` literals in `byroredux/src/cell_loader/terrain.rs`
    and `byroredux/src/cell_loader/terrain_lod.rs`: `0.04`, `4.0`.
- **Status**: NEW. #3073 introduced `DEFAULT_PARALLAX_HEIGHT_SCALE` /
  `DEFAULT_PARALLAX_MAX_PASSES` and #3912 applied the same doctrine to the glass
  scalars, but these sites were not converted.
- **Description**: `collect_static_mesh_draws` uses the named constants
  (`crates/core/src/ecs/components/material.rs`) for its no-`MaterialTextureHandles`
  fallback. #3912 was fixed on the stated rule that "a canonical retune can't leave
  the no-material fallback on the old number". The remaining literal copies are
  exactly that hazard for the parallax pair and the particle IOR.
- **Evidence**: `pub const DEFAULT_PARALLAX_HEIGHT_SCALE: f32 = 0.04;` against the
  literals above. `grep -rn "parallax_height_scale: 0.04"` finds the production
  hits listed; the remaining hits are tests, the Cornell harness and a console
  test helper.
- **Impact**: None today. A retune of the canonical default would silently leave
  the neutral material slot 0, particles and near/LOD terrain on the old value.
- **Related**: #3073, #3912, REN-2026-09-05-D6-02
- **Suggested Fix**: Replace the literals with the three named constants.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other games' arms)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs`, per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
