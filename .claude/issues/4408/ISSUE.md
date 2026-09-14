# #4408 — NIFAL-D6-2026-09-14-02: Stale comment says physics ignores `GlobalTransform::scale` and the fallback bakes `final_scale` into trimesh verts — both false since #3064/#2860

**Labels**: low,nifal,physics,documentation,doc-rot
**Filed from**: `docs/audits/AUDIT_NIFAL_2026-09-14.md` via `/audit-publish` (2026-09-14)

**Source**: `docs/audits/AUDIT_NIFAL_2026-09-14.md` (`/audit-nifal`, HEAD `7374634f5`)

- **Severity**: LOW (doc; code correct)
- **Dimension**: Collision
- **Tier Violated**: no-fabrication (comment states the opposite of the live "scale applied once" contract)
- **Game Affected**: all games reaching the Architecture trimesh fallback (mainly FO4/FO76/Starfield)
- **Location**: `byroredux/src/cell_loader/spawn/mesh_instance.rs:1285-1289`
- **Status**: NEW (a sibling copy that #3064's and #3961's comment sweeps missed; carried in by refactor `a0a52bc3f`)
- **Description**: The comment says the physics sync ignores scale and that this site bakes `final_scale` into verts. In fact `spawn_trimesh_collider_ghost` puts scale on `Transform`/`GlobalTransform`, and `crates/physics/src/convert.rs` applies it exactly once.
- **Evidence**: The orchestrator confirmed the comment text at `:1285-1289`. `git log -S "ignores scale — bhk shapes bake"` traces it to `15016ee02`, which predates #3064.
- **Impact**: This is the third stale copy of the wrong contract that already caused two scale² bugs (#3064, #3959). Following it would reintroduce `XSCL²` Architecture colliders.
- **Related**: #3064, #3959, #3961, #2860.
- **Suggested Fix**: Replace the lines with a pointer to the live contract in `docs/engine/physal.md`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other load paths)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/emitter.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→canonical boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
