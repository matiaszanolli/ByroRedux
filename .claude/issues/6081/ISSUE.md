# SAVE-SD-01: A load refused at its pre-teardown preflight has already replaced four live ledgers with the save's

Labels: medium,save-load,bug

**Severity**: MEDIUM · **Dimension**: Live Load-Apply · **Data-Loss Class**: silent-drop
**Location**: `byroredux/src/save_io/live_load.rs:512-522` (pre-reload restore) versus `:85-97` and `:220-238` (the preflights) and `:540-546` (`None` -> `return`)

## Description
`execute_pending_save_loads` restores `PRE_RELOAD_RESOURCES` (`ReferenceEnableState`, `ReferenceLockState`, `ReferenceScriptState`, `ScriptedFactionChanges`, `live_load.rs:37-42`) before the reload. The two reload helpers then run their non-destructive preflights (`validate_cell_loadable`, `build_exterior_world_context`) and, on failure, `return None` with the message "keeping the current cell/session (on-disk save intact)". The ledgers were already overwritten by the save's values (`restore_resources_subset` -> `World::insert_resource`) and nothing puts the live values back. The same holds for the "snapshot lost its cell/exterior context" arm (`:533-538`) and for a post-teardown reload failure.

## Evidence
Order in the drain is `restore_resources_subset(PRE_RELOAD_RESOURCES)` (`:512`) -> `without_parked_state(... reload_*_session ...)` (`:528`); `reload_interior_session` preflight failure returns `None` at `:96`; the exterior one at `:235`. `live_reload_tests.rs` pins the restore-before-reload order (`saved_resources_are_restored_before_the_cell_reload`) but nothing covers the abort path. `without_parked_state` protects only `PersistentReferenceStates` and `StreamStateSnapshots`.

## Impact
After a refused load (ESM moved, cell editor id renamed, mod set changed) the player keeps playing in the live world, but the Papyrus `Enable`/`Disable` ledger, scripted lock overrides, once-only trigger parking and scripted faction memberships are now the *other timeline's*. They govern the next cell respawn, and the next quicksave persists them, so the live session's own entries are lost for good. Reachable only when a preflight fails, hence MEDIUM.

## Related
#3789, #4135, #5919 (which added `ScriptedFactionChanges` to the pre-reload set).

## Suggested Fix
Run the two preflights before `restore_resources_subset`, or snapshot the four live values and reinstate them when the outcome is `None` before teardown. Add a test that aborts a load and asserts the ledgers.

Source: docs/audits/AUDIT_SAVE_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other per-placement components absent from the eviction row)
- [ ] **TESTS**: A regression test pins this specific fix
