# GAME-D7-2026-10-31-02: `raise_actor_attach_story_event` was inserted between `raise_kill_story_event` and its doc block

Labels: low,gameplay,documentation,doc-rot

Filed from `docs/audits/AUDIT_GAMEPLAY_2026-10-31.md` (finding `GAME-D7-2026-10-31-02`).

- **Severity**: LOW (doc-rot)
- **Dimension**: Player Feedback & Flow / Story Manager producers (Dim 7)
- **Location**: `byroredux/src/systems/story_events.rs:370-381`
- **Status**: NEW (`4e6132a73`)
- **Description**: The `#5434 KILL` doc comment (lines 370-377) now runs straight into the `M43 OAAT` doc comment, so rustdoc attaches both to `raise_actor_attach_story_event` and `raise_kill_story_event` (line 408) has no documentation. The `KILL` text states the slot contract (R1 victim, R2 killer, L1 session LCTN) that the five death producers rely on.
- **Impact**: Wrong generated docs on two functions. Same shape as ESM-2026-10-31-04.
- **Suggested Fix**: Move the `OAAT` function above the `#5434` doc block, or re-attach the block to `raise_kill_story_event`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other games / siblings)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
