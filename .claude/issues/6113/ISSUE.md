# SKY-SD-02: Pre-baked FaceGen, facetint and the PNAM fallback key off the placed shell record, not the "Use Traits" terminal

Labels: medium,character,bug,game:skyrim,legacy-compat

## Description
`prepare_prebaked_state` (`byroredux/src/npc_spawn/resumable/prebaked.rs`) resolves the Use-Traits terminal (`resolved.r#traits`) and uses it for race, gender, skeleton path and `WNAM` skin. The three face inputs are read from the **shell** instead: the facegeom NIF path (`prebaked_facegen_nif_path(plugin_name, npc.form_id)`, line ~146), the facetint DDS path (line ~147/148), and the NPC's PNAM head parts (`prebaked_head_fallback_paths(npc, ...)` reads the shell's `npc.face_morphs`). For an NPC whose `ACBS` template flags set Use Traits (0x01), the face belongs to the template ("Traits tab, including race, gender, height, weight, voice type... Character Gen tabs", UESP *Skyrim Mod:Mod File Format/NPC*, template flag 0x01), and the CK does not bake facegeom for the inheriting record. The caller (`byroredux/src/cell_loader/references/mod.rs`) passes `plugin_for_form_id(child_form_id)`, i.e. also the shell's owner.

`#4457`'s `ResolvedNpc` hoist and its pinning test `resolve_inherited_call_sites_are_enumerated_and_pinned` count only calls of `resolve_inherited_*`, so a field read straight off the shell (`npc.form_id`, `npc.face_morphs`) is invisible to the guard, the recurrence that issue was meant to end. (#5359 is the WNAM skin, #4133/#4457 the TPLT hoist, #1658 inventory inheritance; none cover the face.)

## Evidence (static)
- `Skyrim.esm`: 5,118 `NPC_`; 2,053 have Use Traits set with a `TPLT`. **Only 2 of those 2,053 have their own facegeom.** The other 2,051 would need the terminal's.
- Walking each chain to its terminal: 460 terminals are `NPC_` records with a shipped facegeom (humanoids), 475 are facegeom-less terminals (creatures), 1,118 chains end in an `LVLN` that `walk_inherited_records` resolves at spawn.
- Placed references: of ~10,500 non-deleted `ACHR`, 7,757 point at a Use-Traits base. 351 resolve directly to a humanoid terminal with facegeom (`TreasCorpseBandit*`, `CWSiegeSons/ImperialArcher*`, `TG02Mercenary01`), 634 to a creature terminal, 6,773 through an `LVLN`. Of the 2,738 NPC entries in the 527 `LVLN` lists, 651 are Use-Traits shells and 370 of those terminate on a record with authored facegeom.
- After #5408 the miss is no longer headless: the fallback adds the race base head (`NordRace` male: `MaleHeadNord`, `MaleMouthHumanoidDefault`, `MaleEyesHumanLightBlue`, `HairMaleNord10`). So every bandit, guard and soldier of a race gets the same face and hair, while the template's authored face is never loaded. The shell's own PNAM list (432 Use-Traits records carry one) is ignored by the game for these actors, yet is what the fallback reads.

## Impact
Roughly 40% of NPC base records, including the `LVLN`-driven encounter population (by editor id: 285 of 528 `*Bandit*` and 363 of 413 `*Guard*` records are Use-Traits; 135 bandit, 25 vampire and 18 Forsworn records end on an `NPC_` terminal with authored facegeom, and 353 guard records on an `LVLN` template), render a generic race head with no authored face, no facetint and the wrong hair. Visual, not a crash. It also makes the #5408 fallback, designed for the one vanilla record without facegeom (the player), the steady path for a large share of the world.

## Related
CHAR-SD-01 (same family on the runtime-FaceGen arm: gender read from the shell; different fix site, filed separately), FO4-SD-01 (plugin-local FormID segment of the same path builder; the plugin segment must also come from the terminal's owner), #4457, #5359, #5408.

## Suggested Fix
Derive the facegeom id, the facetint id, the owning plugin segment and the PNAM list from `resolved.r#traits` (the terminal) instead of `npc`, and pass the load order (or a pre-resolved plugin name for the terminal) into `prepare_prebaked_state`, since `plugin_name` currently comes from the shell. Extend the call-site pin test to catch `npc.form_id` / `npc.face_morphs` reads in the prebaked arm. Add a unit test: a Use-Traits shell whose terminal has a facegeom resolves that path.

Source: docs/audits/AUDIT_SKYRIM_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other readers/builders of the same field, other games sharing the code path)
- [ ] **TESTS**: A regression test pins this specific fix
