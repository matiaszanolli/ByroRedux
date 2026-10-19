# OBL-SD-01: Oblivion humanoid idle clip path points at a file Oblivion does not ship; the idle pool is empty

Labels: high,animation,bug,game:oblivion,legacy-compat

**Severity**: HIGH · **Dimension**: 3 (NPC spawn / animation) · **Game**: Oblivion

## Description
`humanoid_default_idle_kf_path` has the arm `GameKind::Oblivion | GameKind::Fallout3NV => meshes\characters\_male\locomotion\mtidle.kf`, sharing FNV's path. Oblivion ships no `locomotion\` directory; its standing idle is `meshes\characters\_male\idle.kf`. The sibling `humanoid_walk_kf_paths` (`npc_spawn.rs:~890`) already documents "Oblivion has no `locomotion\` directory at all (BSA scan 2026-09-18: zero hits)", but the idle function was not split the same way. The idle pool is therefore empty for every Oblivion humanoid and no clip drives a standing NPC.

## Evidence
Location: `byroredux/src/npc_spawn.rs:1028-1034` (`humanoid_default_idle_kf_path`), consumers `:719-729` (`load_idle_clip`), `:863` (`load_idle_pool`), `byroredux/src/cell_loader/references/mod.rs:420-421`, `byroredux/src/player_body.rs:389`, `byroredux/src/systems/cinematic.rs:144`; vacuous pin `byroredux/src/npc_spawn/tests.rs:899-908`. Re-confirmed present at HEAD (the arm still returns the `locomotion\mtidle.kf` path for Oblivion).

Parsing `Oblivion - Meshes.bsa` (20,182 names) and every DLC/Knights archive: `meshes\characters\_male\locomotion\mtidle.kf` absent, no path containing `\locomotion\` or `mtidle` in any of the 17 archives; `meshes\characters\_male\idle.kf` present (25,817 B, a `NiControllerSequence` with B-spline `Bip01` channels and an `IdleK...` text key), `handtohandforward.kf` present. `load_idle_clip` -> `load_kf_clip_by_path` returns `None`, so `load_idle_pool` returns an empty `Vec`; `Finalize` (`npc_spawn/resumable/runtime.rs:~860`) then calls `pick_idle_handle(&[], form_id)` and inserts no `AnimationPlayer`. The walk clip works (`handtohandforward.kf`), but `npc_walk_animation_system` removes the inserted player after a sustained stop (`RemovePlayer`), so a stopped NPC holds its last sampled pose and a never-moving NPC holds the bind pose. `installed_oblivion_body_paths_exist_in_the_meshes_archive` covers body paths only; `idle_kf_path_only_for_kf_era_games` asserts `is_some()` for Oblivion, which passes for a path that does not exist.

## Impact
Every standing Oblivion humanoid (NPC_, the player body's idle, and the cinematic generic idle) is unanimated. The most visible Oblivion-only defect in the area, in every populated interior and city. Not confirmed on-device (no engine run); backed by archive measurement and code reading.

## Related
#794, #772 (FNV-era fixes this path was copied from; closed), #5495 (creature seating, separate), FNV-SD-04 (creature clip resolver, same family).

## Suggested Fix
Split the arm (`Oblivion => meshes\characters\_male\idle.kf`, FO3/FNV keep `mtidle.kf`) and add `humanoid_default_idle_kf_path(Oblivion)` to the installed-archive test next to the body-path assertions, so a non-archived idle fails the lane instead of passing on `is_some()`.

Source: docs/audits/AUDIT_OBLIVION_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / the sibling games' paths (FO3, FNV, Oblivion share this path)
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the parser / spawn boundary, never pushed into shaders or the renderer
- [ ] **TESTS**: A regression test pins this specific fix (real-data `--ignored` test where the defect is a data census)
