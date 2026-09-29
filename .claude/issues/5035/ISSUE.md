# #5035 — ECS-2026-09-29-D5-01: Two exclusive Access rows drifted from their bodies in the dialogue commits (interaction_system, npc_dialogue_selection)

**Labels**: low, bug, ecs, concurrency, dialogue

**Source**: `docs/audits/AUDIT_ECS_2026-09-29.md` — finding `ECS-2026-09-29-D5-01`

**Severity**: LOW

**Dimension**: 5 — Scheduler Wiring

**Location**:
- `byroredux/src/boot/schedule/update.rs:107-153` (`interaction_system`)
- `byroredux/src/boot/schedule/late.rs:426-441` (`npc_dialogue_selection`)

**Status in report**: NEW. The same class as #4821 (closed).

## Description

- The `interaction_system` row says it "covers the system body and the helpers it calls directly". `ab31cfefe`'s Npc arm (`interaction.rs:1256-1292`) now reads `ActorValues`, plus the `SceneQuestAliasRegistry`, `SceneActorBindings` and `QuestStageState` resources through `running_quests_binding_entity`. None of these is declared; `SceneAliasCandidate` and `Dead` were already listed.
- `npc_dialogue_selection`'s row lists `apply_selection`'s writes, `DialogueRegistry` and `NpcDialogueTopic`. `766e1746e` added a third write, `try_resource_mut::<DialogueSurfaceState>()` (`npc_dialogue.rs:151`), which is undeclared.

## Evidence

`git show ab31cfefe -- byroredux/src/boot/schedule/` adds no rows to `interaction_system`, and `git show 766e1746e --stat` touches no schedule file.

## Impact

The analyzer never pairs exclusives, so no conflict is hidden today. The rows are the promotion baseline, and the reference the recorded `BYRO_LOCK_ORDER_CHECK` edges are compared against. The mechanical guard cannot see either system, because both are outside `PARALLEL_SYSTEMS` and outside the three covered exclusive fns.

## Related

Dialogue-landing cluster (`ab31cfefe` / `766e1746e`), filed as distinct issues: ECS-2026-09-29-D1-01 (#5025), ECS-2026-09-29-D1-02 (#5032), ECS-2026-09-29-D7-01 (#5038), SCR-D3-2026-09-29-01 (#5041), LC-D3-01 (#5045), ESM-2026-09-29-D2-03 (#5048). The per-frame Talk-candidate scan on the same code (ECS-2026-09-29-D6-01 = PERF-D1-2026-09-29-01) is tracked under open #3475.

## Suggested Fix

- `interaction_system`: add `.reads::<ActorValues>()`, `.reads_resource::<SceneQuestAliasRegistry>()`, `.reads_resource::<SceneActorBindings>()` and `.reads_resource::<QuestStageState>()`. If D1-01 or D6-01 reshapes the arm, declare what the new shape reads.
- `npc_dialogue_selection`: add `.writes_resource::<DialogueSurfaceState>()`.

Validated at HEAD 9fcfdc3fc: `byroredux/src/boot/schedule/update.rs` `interaction_system` row declares `SceneAliasCandidate`/`Dead` but not `ActorValues`, `SceneQuestAliasRegistry`, `SceneActorBindings` or `QuestStageState`; `boot/schedule/late.rs` `npc_dialogue_selection` row has no `DialogueSurfaceState` while `apply_selection` calls `try_resource_mut::<DialogueSurfaceState>()`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other exclusive rows touched by `ab31cfefe` / `766e1746e` / `0182fc5e8`)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
