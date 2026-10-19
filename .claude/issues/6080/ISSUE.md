# CHAR-SD-01: The runtime-FaceGen spawn arm (Oblivion/FO3/FNV) takes gender from the shell record, but the Female flag is a Use-Traits field and the race beside it comes from the Use-Traits terminal

Labels: medium,character,bug,game:fnv,game:fo3

## Description
`prepare_runtime_state` (the runtime-FaceGen spawn arm used by Oblivion / FO3 / FNV) resolves the NPC once (`spawn_placement_root`) and takes the race from `resolved.traits` (the Use-Traits terminal), but derives `gender` from the **shell** record's ACBS: `Gender::from_acbs_flags(npc.acbs_flags)`. That `gender` selects the body meshes (`humanoid_body_paths`), the head-part gender section (`want_gender_tag`), the gendered armor models inside `build_npc_equip_state`, and is persisted as `ActorBodyClass.gender` for mid-life gear imports. The pre-baked arm and `npc_skeleton_path` read the same flag from the traits terminal (`traits.acbs_flags`), so the two arms disagree for the same record shape.

The Female flag is a Use-Traits field: xEdit `wbDefinitionsFNV.pas:6828` `.SetFlagHasDontShow(0, wbActorTemplateUseTraits)` on ACBS flag bit 0 `'Female'`; `ResolvedNpc::traits` is documented as "race, ACBS gender flags, skeleton" (`equip.rs:677-678`). Same defect class as #4092 / #4457, which fixed the race half.

## Evidence
- Location: `byroredux/src/npc_spawn/resumable/runtime.rs:222`. Correct siblings: `resumable/prebaked.rs:102` and `npc_spawn.rs:517-518` (`traits.acbs_flags`). Also `runtime.rs:302` logs the shell's `npc.race_form_id` rather than the resolved race, and `runtime.rs:475` (creature arm) reads the shell as well (harmless there). Re-validated against current code: the three reads are still present.
- Read-only census of the shipped masters: FalloutNV.esm has 744 `NPC_` shells with Use Traits + `TPLT`; 127 point at a direct `NPC_` (0 differ in the Female bit) and 617 point at an `LVLN`. Using the code's own deterministic pick (highest eligible level, last on a tie, no nested lists), **156 of the 617** LVLN-templated FNV shells have a Female bit different from the picked template. Fallout3.esm has 164 LVLN-targeted shells, 1 differs. Approximate (nested lists not modelled). The real-engine pick is random, which only makes the template the sole stable gender source.
- The `resolve_inherited_call_sites_are_enumerated_and_pinned` guard cannot see this: it counts `resolve_inherited_*` calls, not raw `npc.<field>` reads of governed fields.

## Impact
A shell flagged female whose picked template is male (or the reverse) spawns with the template's race and Background but the other gender's body, head parts and gear models. Visual/identity inconsistency, no crash; confined to LVLN-templated FNV shells (about a quarter) and almost none on FO3. Oblivion has no `TPLT`; Skyrim/FO4 take the correct path.

## Related
#4092, #4457, #4137 (CHAR-SD-03, existing: stale `template_flags` doc), SKY-SD-02 (same family on the pre-baked arm: facegeom/facetint/PNAM read from the shell; different fix site, filed separately). Outside this area and not verified for templated shells: `systems/dialogue_voice.rs:204,416`, `crates/scripting/src/anim_subgraph.rs:76`, `crates/scripting/src/actor_base_traits.rs:50` also read the shell's `acbs_flags` for sex.

## Suggested Fix
Use `resolved.r#traits.acbs_flags` at `runtime.rs:222` (the creature arm's `:475` can follow for uniformity), log the resolved race at `:302`, and add a spawn-boundary test with a female shell over a male template asserting `ActorBodyClass.gender` on both arms.

Source: docs/audits/AUDIT_CHARACTER_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other readers/builders of the same field, other games sharing the code path)
- [ ] **TESTS**: A regression test pins this specific fix
