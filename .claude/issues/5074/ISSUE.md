# #5074 — SCR-D5-2026-09-29-01: running_quests_binding_entity's doc says a missing QuestStageState means the quest never appears; the code treats every installed quest as running

**Labels**: low, documentation, doc-rot, scripting, dialogue

**Source**: `docs/audits/AUDIT_SCRIPTING_2026-09-29.md` — finding `SCR-D5-2026-09-29-01`

**Severity**: LOW

**Dimension**: Scene/Package/Dialogue

**Untrusted-Input**: No

**Location**:
`crates/scripting/src/scene/quest_alias.rs:908-915` (the doc) vs `:923-928`
(`running.as_ref().is_none_or(|stages| stages.is_running(*quest))`).

**Status in report**: NEW. Routed from `/audit-concurrency` (the "Routed" section of `AUDIT_CONCURRENCY_2026-09-29.md`) and verified.

## Description

the code follows the crate's deliberate data-only convention. `refresh_scene_actor_bindings`
(`:709-718`: "the live engine always installs QuestStageState") and `quest_alias_diagnostics` (`:180-182`) do the same.
The doc is wrong, not the code.

## Impact

none in production. The doc is the only contract P4 callers have for this function.

## Suggested Fix

reword `:914-915` to say that a quest with no installed alias definition never appears, and that with
no `QuestStageState` resource (tool and test worlds) every installed quest counts as running.

Validated at HEAD 9fcfdc3fc: the doc on `running_quests_binding_entity` (`crates/scripting/src/scene/quest_alias.rs`) still says a quest with "no `QuestStageState` resource … never appears", while the body filters with `running.as_ref().is_none_or(|stages| stages.is_running(*quest))`.
