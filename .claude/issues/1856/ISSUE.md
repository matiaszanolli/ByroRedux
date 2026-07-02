# FO3-D1-01: FO3 WaterShaderProperty.water_shader_flags dead-ends at MaterialInfo

**Severity**: LOW
**Dimension**: FO3 Rendering Path (Inline Shaders)
**Location**: `crates/nif/src/import/material/walker.rs:583-591,962-964`; `crates/nif/src/import/material/mod.rs:730-741`; `crates/nif/src/import/types.rs`
**Status**: NEW
**Labels**: low, nif-parser, legacy-compat, bug
**Source audit**: `docs/audits/AUDIT_FO3_2026-07-02.md`
**GitHub issue**: https://github.com/matiaszanolli/ByroRedux/issues/1856

## Description
`MaterialInfo.water_shader_flags` is captured from `BSWaterShaderProperty`
(`walker.rs:590`), but the FO3/FNV non-BS `WaterShaderProperty` branch
(`walker.rs:962-964`) forwards only `env_map_scale` and does not even set
`water_shader_flags`. More broadly, `water_shader_flags` is a field on
`MaterialInfo` but NOT on `ImportedMesh` nor the ECS `Material`, so the
captured value is dropped at the `MaterialInfo → ImportedMesh` boundary.

## Evidence
`grep -rln water_shader_flags` returns only the parser + walker + its own
tests — zero consumers past `MaterialInfo`. `water_shader_legacy_tests.rs`
asserts only `env_map_scale` reaches `MaterialInfo` for the FO3/FNV
`WaterShaderProperty`.

Confirmed against current source (2026-07-02):
- `walker.rs:590`: `info.water_shader_flags = shader.water_shader_flags;` (Skyrim+ `BSWaterShaderProperty` branch only)
- `walker.rs:962-964`: FO3/FNV `WaterShaderProperty` branch sets only `info.env_map_scale = shader.shader.env_map_scale;` — no `water_shader_flags` assignment
- `crates/nif/src/import/types.rs`: `ImportedMesh` has no `water_shader_flags` field
- `byroredux/src/material_translate.rs`: `translate_material` never references `water_shader_flags`

## Impact
Minimal for FO3. FO3 mesh-driven water (`meshes/water/*.nif`) renders as a
`material_kind = 0` lit surface with the authored `env_map_scale`; the
authored reflection/refraction/cubemap intent in `water_shader_flags` is
silently unused. This is **NOT** a dedup collapse (water ≠ glass by
`material_kind`) and **NOT** a divergent `Material` out of the NIFAL
boundary — it is a capture-with-no-consumer gap. There is no visual
regression today because the renderer has no legacy-mesh-water route for
the flag word to land in.

## Related
WATAL water-layer work (`docs/engine/watal.md`); adjacent to but distinct
from #1243 (`WaterShaderProperty` → distinct `GpuMaterial`).

## Suggested Fix
When a legacy-mesh-water render route lands, plumb
`MaterialInfo.water_shader_flags` through `ImportedMesh` and set it on the
FO3/FNV `WaterShaderProperty` branch too (currently only the Skyrim+
`BSWaterShaderProperty` branch populates it). Until then, no action
required — documented so a future audit does not refile it as a
`translate_material` divergence (it is not; the value never reaches
`translate_material`).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
