# SCR-SD-02: `mark_cell_reset` drops a cell's pending `OnReset` at the first scripted reference it sees, and never drops it for a cell with none

Labels: medium,scripting,bug

**Severity**: MEDIUM · **Dimension**: Legacy ObScript (loader-fed state)
**Location**: `crates/scripting/src/obscript_state.rs:187-220` (`mark_cell_reset`), fed by `byroredux/src/cell_loader.rs:637` (`note_player_entered_cell`), called at cell-root creation in `load.rs:571`, `exterior.rs:1156` and `exterior.rs:2033`
**Introduced by**: `d1b298921` / `da2676b70` (OnReset). No open or closed issue found (searched "OnReset", "PendingCellResets").

## Description
`note_cell_entered` pushes the cell onto `PendingCellResets` when the 72-hour (or `iHoursToRespawnCell`) expiry has passed. `mark_cell_reset` runs in Update and, per pending cell, marks every `ObScriptInstance` entity whose `CellRoot` resolves to that cell's `CellFormId`, then drops the entry if it marked at least one (`found`). It retries next frame only when it marked none. Two consequences:
1. **Partial load.** References spawn in budgeted slices across frames (`exterior.rs:2263`, `load.rs:652`), and `stamp_cell_root_range` stamps per slice (`exterior.rs:2276`). The first frame in which any scripted reference exists in the cell consumes the entry. Scripted references spawned in later slices of the same load never receive `OnResetEvent`. The doc comment says "keeps its entry until it has references", which is the defect.
2. **No scripted reference at all.** A cell with no `ObScriptInstance` (most exterior cells, and every cell on Skyrim/FO4, where the query is absent) never sets `found`, so the entry is re-pushed forever. Each frame rebuilds the `scripted` list and does two lookups per instance per stuck cell. When such a cell is later reloaded legitimately, the stale entry resets its scripted references although the cell was not expired on that visit.

## Evidence
`unresolved.push(cell)` at `obscript_state.rs:211-213`, re-extended into `PendingCellResets` at `:215-219`; nothing else drains the resource (grep: only `mark_cell_reset` and `note_cell_entered` touch it). The only test (`obscript_objects.rs:1396-1417`, `on_reset_runs_when_the_cell_expired_between_visits`) puts `CellFormId` on the scripted entity itself, spawns it before the mark, and never loads a second slice or a scriptless cell. The stuck-entry cost was not measured on game data; it is `stuck cells x resident scripted references x 2` lookups per frame, bounded by the number of distinct cells expired in a session.

## Impact
`OnReset` is documented in the module as the trap and animating-object reset. On a multi-frame cell load some of those objects do not reset. In a long session (more than 72 game hours between visits) permanent per-frame work accumulates on FNV/Oblivion, and a later visit can fire a spurious reset.

## Related
#6071 (GAME-SD-01, script state not carried across eviction), `AUDIT_SAVE_2026-10-19.md` (`CellVisits` is saved).

## Suggested Fix
Key the pending entry to the cell-load completion (the job's `Complete` step raising the reset for the whole cell), or stamp `OnResetEvent` at attach time when the cell is flagged expired, instead of polling for "any scripted reference". Drop an entry whose cell root has finished loading regardless of whether it held scripted references.

Source: docs/audits/AUDIT_SCRIPTING_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other per-placement components absent from the eviction row)
- [ ] **TESTS**: A regression test pins this specific fix
