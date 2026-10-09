# #5432: GAME-D2-2026-10-08-03: Dialogue voice segments are fire-and-forget — changing topic, closing or the speaker dying does not stop the line

**Labels**: low,gameplay,dialogue,audio,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5432

**Source**: `docs/audits/AUDIT_GAMEPLAY_2026-10-08.md` — `GAME-D2-2026-10-08-03` (HEAD `00f580e09`)

- **Severity**: LOW
- **Dimension**: Dim 2
- **Location**: `byroredux/src/systems/dialogue_voice.rs:183-196`; `byroredux/src/systems/npc_dialogue.rs:319-396, 514-534`
- **Status**: NEW
- **Description**: `play_line_voice` schedules every response segment as `play_oneshot` with a delayed start and keeps no handle. When `apply_selection` replaces the line (topic click) or `end_open_conversation` closes it, the old segments, including ones not yet started, keep playing over the new line. This is inconsistent with Phase L's line-lifetime model.
- **Suggested Fix**: Keep the line's sound handles on the dialogue state and stop them in `apply_selection`'s outgoing-line path and in `end_open_conversation`.

## Completeness Checks
- [ ] **SIBLING**: other delayed-start oneshot users checked for lifetime ownership
- [ ] **TESTS**: A regression test pins this specific fix
