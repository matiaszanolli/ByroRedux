# #6238: SAVE-2026-10-21-02: The load's own cell reload records the visit against the outgoing session's clock, so an in-process load can owe the loaded cell a spurious `OnReset`

**Labels**: medium,save-load,scripting,bug

**Source**: docs/audits/AUDIT_SAVE_2026-10-21.md · **Severity**: MEDIUM · **Dimension**: Live Load-Apply · **Location**: `byroredux/src/cell_loader/load.rs:581`

- **Data-Loss Class**: corruption-on-load
- **Location**:
  - `byroredux/src/cell_loader/load.rs:581`, `byroredux/src/cell_loader/exterior.rs:1170`, `:2205` (`note_player_entered_cell` inside the reload).
  - `byroredux/src/cell_loader.rs:636-645`.
  - `crates/scripting/src/obscript_state.rs:155-158`, `:200-215` (`CellVisits::enter`, `note_cell_entered` → `PendingCellResets`).
  - `byroredux/src/save_io/live_load.rs:37-42` (`PRE_RELOAD_RESOURCES`), `:608-620` (the outgoing-session discards).
  - `byroredux/src/boot/schedule/update.rs:522` (`mark_cell_reset`).
- **Status**: NEW
- **Description**:
  1. `reload_interior_session` / `reload_exterior_session` run the ordinary loaders, and each cell root they build calls `note_player_entered_cell`.
  2. That call reads `GameTimeRes` and writes `CellVisits`. Neither is in `PRE_RELOAD_RESOURCES`, so during the reload both still hold the **outgoing** session's values.
  3. If the outgoing session last entered that cell ≥ `iHoursToRespawnCell` (72 h default) before its current clock, `CellVisits::enter` reports the cell as reset, and `note_cell_entered` pushes it onto `PendingCellResets`.
  4. The wholesale `restore_resources` afterwards replaces `CellVisits` with the saved one. `PendingCellResets` is unsaved and none of the post-reload discards touch it. The #5435 / #5864 / #6083 calls cover the CLOC cursor, story events, package reports and unload deliveries only.
  5. The next frames' `mark_cell_reset` (Update) therefore stamps `OnResetEvent` on every scripted reference of the loaded cell, and its object scripts' `OnReset` blocks run.
  6. A fresh-process load never does this, because `CellVisits` is empty at that point, so in-process and cold loads diverge. That breaks the contract #5435 established.
- **Evidence**:
  - `cell_loader.rs:640-644`: `let Some(now) = world.try_resource::<GameTimeRes>()… ; note_cell_entered(world, cell_form, now, respawn)`, called inside `without_parked_state(… reload …)` (`live_load.rs:585-596`), before the post-reload `restore_resources` (`:639`).
  - The allowlist reason (`registry_completeness_tests.rs:802`) says only "drained within a frame or two of the cell loading".
- **Impact**:
  - Trigger: an ordinary in-process load of an older save in a location the current session left ≥ 3 game days ago.
  - `OnReset` blocks typically re-arm do-once variables, re-lock containers and reset activator state. All of those are saved `ScriptVariables` / ledger state, so the spurious reset is persisted by the next save.
  - Only Oblivion/FO3/FNV object scripts (M47.3) carry `OnReset`. Exterior reloads note every tile in the radius, which widens the trigger.
- **Related**: #5435, #6083 (same class: outgoing-session state leaking across the drain), SCR-2026-10-21-05 (a different `PendingCellResets` defect: `PersistentCellApplyJob` never retires its entry).
- **Suggested Fix**: in `execute_pending_save_loads`, clear `PendingCellResets` beside `discard_pending_unload_deliveries`. A load is not a cell reset, and the saved `CellVisits` is already authoritative after the restore. Pin it with a `live_reload_tests` case that seeds an outgoing `CellVisits` entry older than 72 h and asserts that no `PendingCellResets` survives the drain.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
