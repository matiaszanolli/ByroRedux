# #5264: NIF-D4-2026-10-05-02: `count_spawnable_nif_lights` still counts the exporter-artifact lights that `spawn_nif_lights` now never spawns, which can suppress the ESM LIGH fallback

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/5264
- **Labels**: low,nif-parser,nif,bug
- **Source**: `docs/audits/AUDIT_NIF_2026-10-05.md` (NIF-D4-2026-10-05-02)

_From `docs/audits/AUDIT_NIF_2026-10-05.md` (2026-10-05 comprehensive audit suite, HEAD a2c24b16e)._

- **Severity**: LOW (latent; no vanilla trigger found)
- **Dimension**: Geometry Handoff (NIF light → ECS spawn)
- **Game Affected**: any game with mod content exported from 3ds Max carrying the default light pair; vanilla Oblivion carriers are not LIGH models
- **Location**:
  - `byroredux/src/cell_loader/spawn.rs:417-427` (count, filtered by `is_spawnable_nif_light` only);
  - `:1143-1163` (the skip, now unconditional, by `is_known_exporter_artifact_light_name`);
  - `:726` / `:803` → `spawn/mesh_instance.rs:1527` (`if spawned_nif_lights == 0 && count == 0` gates the ESM LIGH LightSource).
- **Status**: NEW. Searches for "count_spawnable", "exporter artifact light" and #5189 found no match. Related closed issues: #632 (the gate's origin) and #5189.
- **Description**: before #5189, the artifact skip fired only for the 2nd+ carrier (`world.find_by_name`), so the first carrier spawned and a count of ≥1 was true. #5189 made the skip unconditional but left the count on the old predicate. A LIGH whose model carries only the `__MAX_Default_Light` pair now reports 1-2 "spawned" NIF lights and spawns 0. The ESM-authored LightSource fallback is then suppressed, and the fixture is unlit. This is the #632 failure shape, returned by a different route.
- **Evidence**: the code path above. The probe's 48 Oblivion carriers are statues, hair, ears, vines, menus and citadel architecture. None is a LIGH model, and FO3/FNV have 0 carriers per #5189.
- **Impact**: none on vanilla content. A modded light fixture exported with Max defaults renders dark.
- **Related**: #632, #5189, #3557, #5123; owner is the cell-loader handoff, so cross-link `/audit-renderer`.
- **Suggested Fix**: fold the artifact-name check into `is_spawnable_nif_light`, so that the count and the spawn share one predicate.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
