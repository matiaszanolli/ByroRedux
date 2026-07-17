# SKY-D3-NEW-01: Skyrim+ prebaked NPC spawn has no body-mesh fallback — FaceGeom NIF is head-only, RACE WNAM never parsed

**Severity**: HIGH
**Labels**: high, ecs, legacy-compat, bug
**Location**: `byroredux/src/npc_spawn.rs:1801-1807` (doc comment), `byroredux/src/npc_spawn.rs:1910-1944` (`spawn_prebaked_npc_entity`, facegen-only load, no body load), `crates/plugin/src/esm/records/actor.rs` (`parse_race` — no `WNAM`/skin handling anywhere in the file)
**Source audit**: `docs/audits/AUDIT_SKYRIM_2026-07-16.md` (SKY-D3-NEW-01)

## Description
`spawn_prebaked_npc_entity` (the Skyrim/FO4/FO76/Starfield NPC spawn path) loads exactly three mesh sources: skeleton, the per-NPC FaceGeom NIF, and whatever armor resolves from OTFT/CNTO. Its own doc comment claims "the per-NPC head **and** body in one already-skinned mesh." This was empirically disproven: a real vanilla FaceGeom NIF extracted from `Skyrim - Meshes0.bsa` contains only head/face/hair/eye shapes — no body/torso/hand/arm/leg geometry, matching Bethesda's FaceGen SDK convention (head-only bake). Separately, `RaceRecord`/`parse_race` never reads Skyrim's RACE `WNAM` sub-record (the race's default "naked skin" ARMO every actor implicitly wears beneath other layers) — confirmed via grep, `WNAM` only appears in WRLD-related parsers. Expanding the 6 named BanneredMare control-bench NPCs' real OTFT/CNTO/LVLI data shows Hulda and Mikael resolve to `biped_flags=0x80` (Feet-only) — with no body sub-mesh in the FaceGeom NIF and no RACE-skin fallback, these NPCs have zero mesh source for torso/arms/hands/legs today.

## Evidence
- `npc_spawn.rs:1801-1803` doc comment claims combined head+body mesh — confirmed false against real extracted FaceGeom data.
- `npc_spawn.rs:1910-1944` (step 4) loads only the FaceGeom NIF; step 5's comment explicitly states body suppression is not applied, "premised on a FaceGen body that doesn't exist."
- `grep -rn "WNAM" crates/plugin/` — zero occurrences in `esm/records/actor.rs`.
- Live-ESM OTFT/CNTO/LVLI expansion for Hulda (`00013BA3`) and Mikael (`0001A670`) both resolve to `0x80` Feet-only.

## Impact
Every Skyrim SE/FO4/FO76/Starfield NPC spawned through this path risks missing body geometry wherever OTFT/CNTO doesn't explicitly claim the corresponding biped bit — confirmed on 2 of the 6 control-bench named NPCs in the worst way. The M41 equip smoke test would not catch this (component-count gate only, no geometry-completeness assertion).

## Related
Distinct from (and one layer above) the already-fixed component-population issues #1658/#1560.

## Suggested Fix
Parse RACE `WNAM` into `RaceRecord` and auto-equip the resolved skin ARMO as the lowest-priority layer before OTFT/CNTO apply; or, at minimum, load the race's generic body NIF as an always-present base layer on the prebaked path. Fix the stale "head and body in one mesh" doc comment regardless.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (kf-era `spawn_npc_entity` body-suppression path; confirm all four `uses_prebaked_facegen()` games behave consistently once fixed)
- [ ] **TESTS**: A regression test pins this specific fix
