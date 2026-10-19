# SAVE-SD-02: The load's own cell teardown raises `OnUnload` / `OnCellDetach` deliveries that fire inside the loaded session

Labels: medium,save-load,scripting,bug

**Severity**: MEDIUM · **Dimension**: Live Load-Apply · **Data-Loss Class**: none (state contamination of the freshly loaded save)
**Location**: `byroredux/src/cell_loader/unload.rs:437` (raise) · `crates/scripting/src/fragment/reference.rs:233-281` (queue and `raise_unload_events`) · `:301-310` (`reference_script_event_system` drains it) · `byroredux/src/save_io/live_load.rs:548-555` (the reset block that omits it)

## Description
M47.2 `5ea89f3be` made `unload_cell_inner` call `raise_unload_events(world, &victims)` for every unloaded cell. The save-load reload tears the outgoing cell down through `unload_current_interior` / `drain_streaming_state` -> `unload_cells` -> `unload_cell_inner`, so every scripted reference (and alias script owner) in the outgoing session queues an `OnCellDetach` and an `OnUnload` `Delivery` (keyed by owner FormID, not entity) in `PendingUnloadDeliveries`. The load drain resets the outgoing session's CLOC cursor (#5435), story-event markers (#5435) and package reports (#5864) but not this queue. The first `reference_script_event_system` run after the load drains it and interprets the handlers against the **restored** `QuestScriptVariables` / `QuestScriptObjects` / quest state.

## Evidence
`PendingUnloadDeliveries` is a plain `Vec<Delivery>` resource, `init_resource`d at boot (`registration.rs`), allowlisted as "transient" (`registry_completeness_tests.rs:726`), and never cleared by `execute_pending_save_loads`; `without_parked_state` does not gate `raise_unload_events`. The same class: `ActorWatch` (`reference.rs:527`) keeps the outgoing session's bleedout/combat sets and the player's pre-load location, so the first post-load pass can deliver `OnEnterBleedout` / `OnCombatStateChanged` / `OnLocationChange` (old->new across timelines).

## Impact
A handler that writes script variables, calls `Disable()`, sets a stage or moves a quest runs against the loaded state for every scripted reference of the cell the player loaded from. A normal F9 in the same cell reproduces it. Bethesda does not fire unload events for a load; here it corrupts, in handler-dependent ways, the state the save was meant to restore. MEDIUM because the effect depends on handler content.

## Related
#5435, #5864 (same class, different queues), #6018 (ModEvent registrations never pruned on load).

## Suggested Fix
Add a `discard_pending_unload_deliveries` (and an `ActorWatch` reset) beside `discard_package_reports` in the load drain, after the reload and before `restore_resources`; a source-order test like `load_discards_story_event_markers_pending_from_the_outgoing_session` would pin it.

Source: docs/audits/AUDIT_SAVE_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other per-placement components absent from the eviction row)
- [ ] **TESTS**: A regression test pins this specific fix
