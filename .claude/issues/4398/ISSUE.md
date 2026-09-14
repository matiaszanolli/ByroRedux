# #4398 — NIFAL-D5-2026-09-14-01: Emitter orientation is dropped, so the authored spawn cone is world-axis aligned and #4240's azimuth wedge points the wrong way on any rotated placement

**Labels**: medium,nifal,import-pipeline,bug
**Filed from**: `docs/audits/AUDIT_NIFAL_2026-09-14.md` via `/audit-publish` (2026-09-14)

**Source**: `docs/audits/AUDIT_NIFAL_2026-09-14.md` (`/audit-nifal`, HEAD `7374634f5`)

- **Severity**: MEDIUM (NIFAL row: translatable particle emitter data silently dropped; no content removed)
- **Dimension**: Particles
- **Tier Violated**: parked-not-leak (authored rotation is parsed, then silently dropped; not recorded as a deferral)
- **Game Affected**: Oblivion, FO3, FNV, Skyrim (Starfield N/A). #4240 measured wedge-authoring emitters at FO3 250/422 and FNV 405/1262.
- **Location**:
  - `crates/nif/src/import/walk/emitter.rs:754-757` (flat walker keeps only `.translation`)
  - `crates/nif/src/import/types.rs:1857-1859` (`ImportedParticleEmitterFlat` has no rotation field)
  - `byroredux/src/cell_loader/spawn.rs:1274-1275` (`GlobalTransform::new(world_pos, Quat::IDENTITY, 1.0)`; the sibling fog branch at `:1225` does receive `ref_rot`)
  - `byroredux/src/systems/particle.rs:410-413` and `:488-511` (reads only `g.translation`; the cone is built around world +Y and world +X)
  - `crates/nif/src/blocks/particle.rs:139` (`NiPSysVolumeEmitter.Emitter Object` ref discarded)
- **Status**: NEW (the rotation half of #1333, which fixed translation only; the #4240 commit message acknowledges the gap, and no issue tracks it — confirmed by `gh issue list --search` on "emitter rotation" / "emitter orientation")
- **Description**: Gamebryo's emitter direction is expressed in the emitter's own frame. No orientation reaches the canonical `ParticleEmitter`:
  - The flat import drops the composed NIF rotation.
  - The cell spawn drops the REFR rotation.
  - `particle_system` ignores any rotation on the entity's `GlobalTransform`, so even the loose-NIF path's carefully set `local_rotation` is unused.

  Before #4240 the azimuth was a uniform random draw, so yaw was invisible. With the authored wedge now forwarded, every authored fan aims relative to world +X regardless of how the placement is yawed.
- **Evidence**: See Location. `authored_planar_angle_aims_the_spawn_azimuth` uses an identity host rotation, so nothing pins rotated hosts. The number of placed wedge emitters on non-identity REFR yaw was not measured.
- **Impact**: Directional FX (sparks, steam vents, spray/impact fans, directional dust) spawn toward a fixed world direction instead of the placed object's facing. The error varies per placement, so in-world it looks random.
- **Related**: #1333, #4240, #984 (force-field directions share the gap).
- **Suggested Fix**: Carry the composed NIF rotation on `ImportedParticleEmitterFlat`. Insert `ref_rot × nif_rot` on the billboard emitter transform at `byroredux/src/cell_loader/spawn.rs:1274-1275`. In `particle_system`, rotate the sampled offset and `dir` by `g.rotation`. Pin with a rotated-host variant of the azimuth test. Separately verify whether force-field directions are emitter-local before rotating them.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other load paths)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/emitter.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→canonical boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
