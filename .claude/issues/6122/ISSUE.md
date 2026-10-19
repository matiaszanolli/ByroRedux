# SF-SD-03: RACE skeleton path (`ANAM`) is read for Skyrim only; every non-human Starfield actor and every female spawns on the male human rig

Labels: medium,esm-plugin,gameplay,bug,game:starfield,game:fo4,legacy-compat

## Description
`crates/plugin/src/esm/records/actor/race.rs` reads the RACE skeleton `ANAM` only under `b"ANAM" if game == GameKind::Skyrim && in_skeleton_section` (re-validated: the gate is still Skyrim-only). The pin in `actor/tests.rs` (~lines 36-41) asserts `skeleton_models == ["", ""]` for Fallout4, Fallout76 and Starfield. With `skeleton_models` empty, `npc_skeleton_path` (`byroredux/src/npc_spawn.rs:517-531`) always falls back to `humanoid_skeleton_path` -> `meshes\actors\human\characterassets\skeleton_facebones.nif` for Starfield. Starfield (and FO4) `RACE` carries the real path in `ANAM`.

## Evidence
- 31 of 31 Starfield `RACE` records carry `ANAM`: `HumanRace` has `Actors\Human\CharacterAssets\skeleton.nif` (male) and `...\Female\skeleton.nif`; `ModelARace` has `Actors\ModelA\CharacterAssets\Skeleton.nif`; also `LaserTurretRace`, `MiniBotARace`, `CritterRace`, `QuadrupedARace`, `SecurityCameraRace` and others.
- Those skeletons ship: `meshes/actors/modela/...`, `laserturret/...`, and `human/characterassets/female/skeleton_facebones.nif` all exist in `Starfield - Meshes01.ba2` / `Meshes02.ba2`.
- Placed-actor races (9,530 `ACHR`): `HumanRace` 7,096, `CCT_DummyRace` 795, `ModelARace` 402, `HumanCorpseRace` 346, `LaserTurretRace` 272, `ParasiteARace` 133, `SecurityCameraRace` 120, `MannequinRace` 90, `MiniBotARace` 73, `CritterRace` 50, `ChildRace` 39, `ModelSRace` 39, `BallisticTurretRace` 32, `TerrormorphRace` 25: roughly 1,300 non-human placements bound to a human rig. Cydonia: 9 of 75.
- FO4 shares it: all 45 `Fallout4.esm` `RACE` records carry `ANAM` (Dogmeat `actors\Dogmeat\...`, ghouls, turrets, the alien); the same gate empties them.

## Impact
Non-human actors skin to the wrong skeleton (bone-name misses), so their bodies are mis-posed or collapsed. Females use the male rig even once the Starfield ACBS decode (SF-SD-02) is fixed.

## Related
SF-SD-02, #5753 (Argonian / Khajiit body-section tail, another per-race RACE section gap).

## Suggested Fix
Widen the `ANAM` arm to the games whose `RACE` carries it (FO4 / FO76 / Starfield; verified here for FO4 and Starfield), and resolve a Starfield human `skeleton.nif` to its `_facebones` sibling when that exists (the comment near `npc_spawn.rs:505-509` requires the face joints). Update the test pin.

Source: docs/audits/AUDIT_STARFIELD_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other readers/builders of the same field, other games sharing the code path)
- [ ] **TESTS**: A regression test pins this specific fix
