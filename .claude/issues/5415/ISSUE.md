# #5415: CONC-D4-2026-10-08-02: The new Update-stage ordering contracts are comments only

**Labels**: low,concurrency,bug,test-gap
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5415

**Source**: `docs/audits/AUDIT_CONCURRENCY_2026-10-08.md` — `CONC-D4-2026-10-08-02` (HEAD `00f580e09`)

- **Severity**: LOW (hardening; mis-ordering costs a one-frame delay, not a hazard).
- **Dimension**: Scheduler Proof Soundness
- **Location**: `byroredux/src/boot/schedule/update.rs:408-409` (`story_change_location_dispatch`, `story_manager_dispatch`) against `:412` (`quest_alias_refresh_system`); `:442` (`forcegreet_system`) against `ambient_ai_package_system` (`:434`) and the Late `npc_dialogue_selection_system`.
- **Status**: NEW. `grep` for `story_manager_dispatch` / `story_change_location` / `forcegreet` in `scheduler_access_tests.rs` and `boot/schedule/mod.rs` finds no test.
- **Description**: The skill asks for a pin for every new single-writer / multi-reader hand-off. Three exist: the SM dispatcher writes `StoryEventAliasFill` and flips `SceneActorBindings` dirty so that the alias refresh "scheduled right after this system" fills `FromEvent` aliases the same frame (`story_manager.rs:526-538`); the CLOC producer must precede the dispatcher; the force-greet system must sit between the ambient package system (installs the directive) and the Late selection (serials the surface once). All three are registration order plus a comment. Siblings are pinned (`billboard_runs_after_camera_follow_in_late`, `footstep_runs_after_camera_follow_in_late`, `player_body_facing_runs_in_update_before_propagation`, …), each added after a real one-frame-stale bug.
- **Impact**: A reorder delays alias fill / greeting by a frame; nothing in the suite would notice.
- **Suggested Fix**: One test in the style of `player_body_facing_runs_in_update_before_propagation` asserting the three relative positions in `access_report()`.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
