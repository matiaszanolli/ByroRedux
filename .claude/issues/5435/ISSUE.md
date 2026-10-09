# #5435: GAME-D7-2026-10-08-02: `StoryLocationCursor` survives an in-process load, so loading a save from another cell raises a CLOC whose "old location" is the pre-load session's

**Labels**: low,gameplay,quests,save-load,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5435

**Source**: `docs/audits/AUDIT_GAMEPLAY_2026-10-08.md` — `GAME-D7-2026-10-08-02` (HEAD `00f580e09`)

- **Severity**: LOW
- **Dimension**: Dim 7
- **Location**: `crates/scripting/src/story_manager.rs:351-353, 382-419`; `byroredux/src/save_io/registry_completeness_tests.rs:507`
- **Status**: NEW
- **Description**: The cursor is unsaved and is installed only when absent. An in-process load therefore keeps the outgoing session's key and LCTN:
  - loading a save in a different cell fires `CLOC` with `L1` = a location the loaded game was never in;
  - loading a save in the same cell fires nothing;
  - a fresh-process load fires with `L1 = None`.

  The allowlist's "a fresh boot/load legitimately re-fires the event" covers only the last case. CLOC-rooted SM nodes see load-path-dependent event data.
- **Suggested Fix**: Reset the cursor (or its `location`) as part of the save-load apply so every load behaves the same, and document whether a load should raise CLOC at all.

## Completeness Checks
- [ ] **SIBLING**: other session-local Story Manager resources reset on load apply
- [ ] **TESTS**: A regression test pins this specific fix
