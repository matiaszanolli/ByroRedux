# #4331 SCR-D7-2026-09-14-03: `materialize_scene_actor_alias_stubs` is a second identity-only actor-stub path that never attaches scripts (sibling of #4112)

**Labels**: low,scripting,quests,bug,game:skyrim
**Source**: `docs/audits/AUDIT_SCRIPTING_2026-09-14.md`

- **Severity**: LOW
- **Dimension**: Engine Attach & Trigger Wiring
- **Untrusted-Input**: No
- **Location**: `byroredux/src/asset_provider/script.rs` `materialize_scene_actor_alias_stubs` (Skyrim-only, hardcoded MQ101 `0x0003372B` / SCEN `0x000BECD4`), called from `byroredux/src/scene/world_setup.rs`
- **Status**: NEW (present at `b3db49fa`; #4112's location and fix cover only `exterior.rs`)
- **Description**: This function materializes MQ101's scene-actor ACHRs (Tullius / Ulfric / Elenwen) via `spawn_logical_quest_reference` plus a `RemoteSceneActorStub` marker, with no `attach_*` call. Base SCRI/VMAD and the ACHR's own VMAD are therefore dropped while the actor is stub-only. Every `spawn_logical_quest_reference` caller in `synth_child.rs` does attach.
- **Impact**: Scoped to one demo scene. Whether these records carry VMAD is **UNVERIFIED**. Neither stub path is counted in `M47.2 scripts:`.
- **Related**: #4112
- **Suggested Fix**: Fix together with #4112 through one shared "logical actor identity + attach" helper. Attach once per `reference_form_id` so a later real load of the home cell doesn't double-attach.

_Source: `docs/audits/AUDIT_SCRIPTING_2026-09-14.md`_

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other primitives / spawn paths / walkers)
- [ ] **TESTS**: A regression test pins this specific fix
