# SAVE-D6-02: destructive teardown before a live load that can fail strands the engine in an empty cell

Labels: bug import-pipeline medium 

- **Severity**: MEDIUM
- **Dimension**: M45.1 Live Load-Apply
- **Data-Loss Class**: reference-break (player stranded in the void; no live-state loss on disk)
- **Location**: `byroredux/src/save_io.rs:518-554` (`execute_pending_save_loads`)

## Description
The drain tears down the current cell (`drain_streaming_state` + `unload_current_interior`) **before** calling `load_cell_with_masters`. If the reload errors (corrupt/missing ESM, renamed cell editor id), the function logs + `return`s — leaving the engine with the old cell already destroyed and no new cell loaded. The on-disk save is untouched (recoverable by relaunch), but the running session is left in an undefined empty-cell state with no in-engine recovery.

## Evidence
Teardown at lines 518-521; `Err(e) => { log::error!(...); return; }` at lines 546-553, after teardown, before any restore. Note `CurrentCellContext` *is* re-validated at drain (`snapshot_cell_context` errors if it vanished between queue and drain — the defensive double-check is present).

## Impact
A `load` of a slot whose cell can't be reloaded drops the player into the void mid-session.

## Suggested Fix
Attempt the reload into a staging area (or validate the cell editor id resolves) before tearing down the live cell; on reload failure, keep the current cell rather than leaving an empty world. At minimum surface a user-visible error rather than only a log line.

## Completeness Checks
- [ ] **TESTS**: A regression test covers the reload-failure path and asserts the prior cell is retained (or a user-facing error surfaces)
