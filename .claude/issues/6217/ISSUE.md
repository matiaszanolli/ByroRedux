# #6217: SCR-2026-10-21-03: The parallel `player_controller_system` writes `KillRecords` (and reads the FormID/alias storages) through the drowning death, undeclared

**Labels**: medium,scripting,concurrency,bug

**Source**: docs/audits/AUDIT_SCRIPTING_2026-10-21.md · **Severity**: MEDIUM · **Dimension**: Fragment Dispatch & Locks (scheduler access) · **Location**: `byroredux/src/boot/schedule/early.rs:21` (`add_to_with_access`, parallel); call path `systems/character.rs:1593-1612` `apply_player_drowning_damage` → `systems/story_events.rs:199` `raise_kill_story_event` → `:185` `raise_death_event` → `crates/scripting/src/obscript_state.rs:501-516` `note_death`; scan list `byroredux/src/boot/schedule/mod.rs:752-768`

**Audit status**: NEW. `note_death` was wired into `raise_death_event` by `d1b298921` (2026-10-18).

**Untrusted-Input**: No

## Description

For #5641 the row declares `StoryEvent` and `OnDeathEvent`. The `PARALLEL_SYSTEMS` scan follows `raise_kill_story_event` inside `story_events.rs`. It does not follow the cross-crate call into `note_death`. `note_death` takes `try_resource_mut::<KillRecords>()` and reads `FormIdComponent`, `SceneAliasCandidate` and `PapyrusPlayerEntity` through `entity_global_form_id`. When the killer is the player, it also writes `MiscStats` and `ChallengeState` and starts magic effects. That branch cannot occur on the drowning path (killer `None`), but it shows how far the hop reaches.

## Evidence

`grep KillRecords byroredux/src/boot/schedule/` returns nothing.

## Impact

There is no conflict today, because no other Early parallel system touches these types and the guard is taken alone. A future Early system that reads `KillRecords` would be scheduled beside it with no ordering. The `every_parallel_system_declares_everything_it_acquires` scan cannot see this hop.

## Related

#5641, #4994 (the explicit cross-file hop list).

## Suggested Fix

Declare `.writes_resource::<KillRecords>()` plus the reads on the `player_controller_system` row. Add `(…obscript_state.rs, "note_death")` to its hop list, or move `note_death` out of `raise_death_event` into the exclusive reconciler.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other PARALLEL_SYSTEMS rows that reach raise_death_event / note_death)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
