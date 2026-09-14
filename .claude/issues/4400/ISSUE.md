# #4400 — NIFAL-D1-2026-09-14-03: MSWP swap re-merge (#4290) leaves the BGEM glass-overlay texture roles on the source sidecar while provenance labels follow the target

**Labels**: low,nifal,renderer,bug
**Filed from**: `docs/audits/AUDIT_NIFAL_2026-09-14.md` via `/audit-publish` (2026-09-14)

**Source**: `docs/audits/AUDIT_NIFAL_2026-09-14.md` (`/audit-nifal`, HEAD `7374634f5`)

- **Severity**: LOW (latent: only BGEM v21+ authors these roles; FO4 ships none)
- **Dimension**: Material
- **Tier Violated**: single-boundary
- **Game Affected**: FO76/Starfield-era BGEM through a REFR material swap; latent on FO4
- **Location**: `byroredux/src/cell_loader/spawn/mesh_instance.rs:189-192` (`textures` seeded from `mesh.material`), `:193-202` (`sources` seeded from the swapped `material`)
- **Status**: NEW (introduced by `82c4450d6`)
- **Description**: #4290 moved every re-resolved role and scalar read onto the swapped material, but the `textures` seed still reads the pre-swap cached material. `glass_roughness_scratch` and `glass_dirt_overlay` never pass through `resolve_effective`, so they keep the source sidecar's maps while `MaterialTextureDebugInfo.sources` reports the target's provenance.
- **Evidence**: The orchestrator re-read `byroredux/src/cell_loader/spawn/mesh_instance.rs:187-202` and confirmed `mesh.material.textures.map_ref` next to `material.textures.zip_map_ref`. The #4290 test asserts only scalars and flags.
- **Impact**: Wrong glass scratch/dirt overlay and mislabelled `mat.dump` provenance on swapped BGEM glass. No current FO4 population.
- **Related**: #4290, #973, #3906.
- **Suggested Fix**: Seed `textures` from `material.textures` at `byroredux/src/cell_loader/spawn/mesh_instance.rs:189`. Extend the #4290 test with a BGEM pair that differ in `glass_roughness_scratch`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other load paths)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/emitter.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→canonical boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
