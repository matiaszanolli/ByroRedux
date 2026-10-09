# #5394: GAME-D7-2026-10-08-01: Story-Manager-started quests lose their event-filled aliases across a save/load — the `StoryEventAliasFill` allowlist's "quests restart through fresh events" is false

**Labels**: medium,gameplay,quests,save-load,bug,game:skyrim
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5394

**Source**: `docs/audits/AUDIT_GAMEPLAY_2026-10-08.md` — `GAME-D7-2026-10-08-01` (HEAD `00f580e09`)

- **Severity**: MEDIUM
- **Dimension**: Gameplay-state coverage (Dim 7)
- **Location**: `byroredux/src/save_io/registry_completeness_tests.rs:508`; `crates/scripting/src/scene/quest_alias.rs:733-800`
- **Status**: NEW (the save-coverage half; the in-process stale-EntityId half is ECS D7-03)
- **Trigger**: Skyrim, an SM-started quest with `ALFE`/`ALFD` event-fill aliases running at save time. The census found 19 such aliases: `WIKill03/04/05/06`, `MGSuspensionQuest`, `DA02KillFriend` (KILL R1/R2/L1) and `DialogueGenericDogHellos` (AHEL R1/R2). Load the save in a fresh process.
- **Description**: The allowlist says the resource needs no save because "after a load the quests restart through fresh events". They do not. The quest is restored as running by the saved `QuestStageState`, and the event that started it (an old kill, an old hello) is never raised again. The alias refresh then runs as follows:
  - it drops every prior binding for registered quests (`resolved.retain(|(quest, _), _| !registered_quests.contains(quest))`);
  - it fills `FromEvent` aliases only from `StoryEventAliasFill`, bypassing the candidate scan by design;
  - with the map empty after a fresh-process load, `story_fills.get(quest)` is `None` and the alias stays unbound.
- **Evidence**: `if let Some(AliasFillType::FromEvent { data, .. }) = alias.fill_type { if let Some(slots) = story_fills.get(quest) { … } continue; }`
- **Impact**: After a load, a running radiant or SM quest's Victim, Killer or speaker aliases resolve to nothing, so stage logic and conditions that read them see `None`.
- **Related**: ECS D7-03 (in-process stale rebind); #5366.
- **Suggested Fix**: Save the per-quest event slots in a FormId-keyed form (e.g. the slots' persistent reference ids), or bind the restored `SceneActorBindings` entries for FromEvent aliases instead of dropping them. Correct the allowlist row.

## Completeness Checks
- [ ] **SIBLING**: other #5366 Story Manager resources' allowlist rows re-checked against load behaviour
- [ ] **TESTS**: A regression test pins this specific fix
