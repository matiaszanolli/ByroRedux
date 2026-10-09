# #5414: CONC-D4-2026-10-08-01: `npc_dialogue_selection`'s Access row omits the `StoryEvent` write and the dialogue-voice resources; the declaration scan cannot follow the cross-file hops

**Labels**: low,concurrency,bug,audio,dialogue
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5414

**Source**: `docs/audits/AUDIT_CONCURRENCY_2026-10-08.md` — `CONC-D4-2026-10-08-01` (HEAD `00f580e09`)

- **Severity**: LOW (informational for an exclusive today; the row is the comparison basis for a future parallel promotion, the same reasoning as #5069 and #5307).
- **Dimension**: Scheduler Access Declarations
- **Location**: `byroredux/src/boot/schedule/late.rs:439-495` (the row); the scan `npc_dialogue_selection_declares_everything_the_spoken_fragment_path_acquires`, `byroredux/src/boot/schedule/mod.rs:1007-1018`; the assert helper `:813-893` and `fn_body` lookup `:785-808`.
- **Status**: NEW. Searched "Access row", "npc_dialogue", "StoryEvent write"; only the closed #5307 (spoken-fragment writes) and open #5069 (a different system) match.
- **Description**: The row declares the quest/fragment writes #5307 added, but not what the two newest hops acquire:
  - `raise_hello_story_event` (`006905d74`, called at `npc_dialogue.rs:702`): `StoryEvent` **write**, plus `CurrentCellContext` / `CurrentExteriorContext` reads (via `resolve_current_lctn`).
  - `play_line_voice` (`f8950e7cc`, called from `apply_selection` at `npc_dialogue.rs:344`): `SoundArchiveProvider` read, `SoundCache` **write**, `AudioWorld` **write**, `LoadedPluginSet` read, `GlobalTransform` read.

  The scan follows callees by `fn_body(src, callee)` within each listed source file only; the listed pairs are `npc_dialogue.rs`, the fragment systems, the fragment effects and `scene.rs`. `dialogue_voice.rs` and `story_events.rs` are not listed, so both hops are invisible and the guard stays green.
- **Impact**: `sys.accesses` understates this system, and a parallel-lane promotion of the row would look safe when it is not (the #5307 failure mode, one wave later).
- **Related**: #5307, #5069 (same class, different row), CONC-D3-2026-10-08-01 (the `StoryEvent` write).
- **Suggested Fix**: Add the declarations, and add `(DIALOGUE_VOICE_SRC, "play_line_voice")` and `(STORY_EVENTS_SRC, "raise_hello_story_event")` to the scan's source list.

## Also reported as `AUD-2026-10-08-D4-01` (AUDIT_AUDIO_2026-10-08.md)

Cross-report duplicate merged at publish time; the sibling report's text follows.

**Source**: `docs/audits/AUDIT_AUDIO_2026-10-08.md` — `AUD-2026-10-08-D4-01` (HEAD `00f580e09`)

- **Severity**: LOW. The system is exclusive, so the under-declaration has no scheduling effect today.
- **Dimension**: Manager, ECS Lifecycle & Schedule
- **Location**:
  - `byroredux/src/boot/schedule/late.rs:437-493`: the Access row.
  - `byroredux/src/boot/schedule/mod.rs:1008-1018`: the guard.
  - `byroredux/src/systems/dialogue_voice.rs:98-195`.
- **Status**: NEW (introduced by `f8950e7cc`)
- **Description**: `play_line_voice`, reached from `apply_selection`, acquires:
  - `AudioWorld` (write) and `SoundCache` (write);
  - `SoundArchiveProvider` and `LoadedPluginSet` (read);
  - the `GlobalTransform` component (read).

  None appears on the row. #5307 fixed exactly this class for the fragment queues and explicitly argued that an under-declaration "would make a parallel-lane promotion of this row look safe when it is not". Its guard scans `npc_dialogue_selection_system_inner`, `apply_spoken_info_fragment`, `apply_fragment_guard_free` and `mark_scene_actor_bindings_dirty`, but not `apply_selection` or `play_line_voice`, so it stays green.
- **Evidence**: `grep -n "AudioWorld\|SoundCache\|SoundArchiveProvider\|LoadedPluginSet" byroredux/src/boot/schedule/late.rs` finds no hit in the `make_npc_dialogue_selection_system` row.
- **Impact**: An `access_report` built on this row under-reports conflicts with `audio_system` (exclusive, write `AudioWorld`) and `make_combat_feedback_system` (writes `SoundCache`).
- **Related**: #5307 (closed), D5-02.
- **Suggested Fix**:
  - Add `.writes_resource::<AudioWorld>()`, `.writes_resource::<SoundCache>()`, `.reads_resource::<SoundArchiveProvider>()`, `.reads_resource::<LoadedPluginSet>()` and `.reads::<GlobalTransform>()`.
  - Extend the source-scan tuple list with `(NPC_DIALOGUE_SRC, "apply_selection")` and the `dialogue_voice.rs` `play_line_voice` body.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other `assert_declares_everything_it_acquires` scans whose callees cross into unlisted files)
- [ ] **TESTS**: A regression test pins this specific fix
