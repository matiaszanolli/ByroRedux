# #5419: ECS-2026-10-08-D7-03: `StoryEventAliasFill` keeps event `EntityId`s for the session and rebinds them after their cell unloads, with no liveness check

**Labels**: low,ecs,scripting,quests,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5419

**Source**: `docs/audits/AUDIT_ECS_2026-10-08.md` — `ECS-2026-10-08-D7-03` (HEAD `00f580e09`)

- **Severity**: LOW. Entity ids are never recycled, so a stale id is a failed lookup, never aliasing. The visible effect is a FromEvent alias that reports as filled but points at nothing. Owner overlap: `/audit-scripting`.
- **Dimension**: 7 — Retained `EntityId` fields
- **Location**:
  - `crates/scripting/src/story_manager.rs:238-249` (the doc), `:531-533` (the write)
  - `crates/scripting/src/scene/quest_alias.rs:769-786` (the consumer)
- **Status**: NEW (introduced by `3ba9f1d5c`, #5366)
- **Description**: The resource's doc says an entry "stays for the session so an alias refresh after a cell reload can still fill". But the slots are session `EntityId`s:
  - When the victim's or killer's cell unloads, its actor is despawned.
  - `unload.rs:514` marks the alias bindings dirty, and `refresh_scene_actor_bindings` re-inserts the recorded `entity` with no `Dead` / liveness / `FormIdComponent` check. Candidate fills do have an `ALLOW_DEAD` check.
  - A reloaded cell gives the same actor a new id, which this path never sees.

  So after a reload the alias is bound to a dead id rather than re-resolved, which is the opposite of what the doc promises.
- **Suggested Fix**: Either store the event references as FormIDs and resolve them through `resolve_entity_by_global_form_id` at refresh, or skip slot entities that are no longer alive and correct the doc to "session-scoped, not reload-stable".

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other session-lived resources holding `EntityId`s across cell unload)
- [ ] **TESTS**: A regression test pins this specific fix
