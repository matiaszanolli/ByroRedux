# SCR-D3-2026-10-17-07: `quest_advance_system` double-fires for two triggerers on one volume in a frame

Issue: #5995
Labels: low, scripting, bug, quests

Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D3-2026-10-17-07: `quest_advance_system` double-fires for two triggerers on one volume in a frame

- **Severity**: LOW
- **Dimension**: Event Runtime
- **Location**: `crates/scripting/src/papyrus_demo/quest_advance.rs:370-376,389-437`
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: The per-entity `signalled` dedup stops ActivateEvent+OnTriggerEnter double delivery, but `triggerers` expands to one `(entity, triggerer)`
  pair each and every passing pair pushes its own `PendingAdvance`; each does `set_stage` (which journals a `QuestStageAdvanced`, `quest_stages.rs:349-362`).
  With an `Any` gate (or `BaseForm` plus a second matching actor) two actors entering in the same frame advance the stage twice - including past
  `onlyOnce`/`disableWhenDone`, whose disarm only applies after the loop.
- **Impact**: the stage fragment can run twice for a fire-once trigger (non-idempotent effects such as AddItem repeat). Needs two simultaneous
  entrants; PlayerOnly gates are unaffected.
- **Suggested Fix**: break after the first passing triggerer of an entity (or dedupe `PendingAdvance` by `(entity, quest, stage)`).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **TESTS**: A regression test pins this specific fix
