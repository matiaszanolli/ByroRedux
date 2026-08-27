# FNV-D2-03

**Issue**: #3336
**Filed**: 2026-08-26 from `docs/audits/AUDIT_FNV_2026-08-26.md`

---

**Severity**: LOW
**Dimension**: 2 — NIFAL Canonical Translation
**Status**: NEW
**Source**: `docs/audits/AUDIT_FNV_2026-08-26.md` (audit HEAD `d6e16c90`)


**File**: `byroredux/src/cell_loader/terrain_lod_btr.rs:284-315`, doc comment at `:105-113`

**Premise verified**: the spawner inserts `Transform`, `GlobalTransform`,
`MeshHandle(…)`, `TextureHandle`, `MaterialTextureHandles` (**including a bound
normal map**, `:296-305`), `WorldBound`, `RenderLayer`, `IsLodTerrain` — and no
`Material`. Its own doc still reads *"LOD entities carry none — that is #2444
(MAT-D3-02). Until it lands…"*, but #2444 **has** landed: `terrain.rs:676`,
`terrain_lod.rs:881`, `object_lod.rs:327` and `placement_lod.rs:522` all route
through the boundary now. `terrain_lod_btr.rs` was not converted, and the
harness that is supposed to prevent exactly this —
`material_translate.rs::every_exterior_spawner_inserts_a_boundary_material`
(`material_translate.rs:1383`) — enumerates only those four files, so the fifth
exterior spawner is invisible to it.

**Impact**: **not FNV-visible.** `.btr` is the Skyrim/FO4 combined LOD
quadtree only (`lod_support.rs:368`, `env_translate.rs:96`); FNV distant
terrain goes through `terrain_lod.rs`. Reported here because it is a live hole
in the NIFAL single-boundary invariant this dimension owns, found while
verifying the FNV side of it. Consequence on Skyrim/FO4: these draws take
`render/static_meshes.rs:354-367`'s no-`Material` arm (hardcoded
`roughness 0.5`, `metalness 0.0`, identity tints) — the exact second
materialization site #2444 was filed to delete — and the authored `_n` normal
map cannot reach the shader with `MAT_FLAG_MODEL_SPACE_NORMALS`.

**Fix sketch**: call `translate_texture_only_material(base_texture_path)` and
`world.insert(entity, material)` in `spawn_btr_quad`, and add
`cell_loader/terrain_lod_btr.rs` to the `every_exterior_spawner_…` table so
the harness actually closes over all five spawners.

---

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
