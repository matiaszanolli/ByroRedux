# GAME-SD-01: Per-reference script state (`TwoStateActivator`, `ScriptVariables`, incl. the new ObScript object-script locals) is not captured at eviction, so every cell re-entry resets it

Labels: medium,gameplay,save-load,scripting,bug

**Severity**: MEDIUM · **Dimension**: Persistence across the stream boundary (Dim 2 / Dim 7)
**Location**: `byroredux/src/cell_loader/reference_state.rs:28-62` (the row has no field for either), `:119-264` (`capture`); respawn stamps `crates/scripting/src/translate/recognizers/two_state_activator.rs:34-52` and `crates/scripting/src/obscript_objects.rs:204-208`

## Description
`ReferenceState` carries inventory, equipment, weapon, actor values, spells, perks, control, bleedout, dead and picked-up. `TwoStateActivator` and `ScriptVariables` are saved components (`save_io/registry.rs:299-300`), but the registry only serialises entities that are *resident* at save time. Eviction despawns the entity (`unload.rs:631`), and the respawn runs the recognizer / `attach_object_script`, which stamp the authored `isOpen` and a fresh `ScriptVariables::default()`. Nothing in `capture`, `stream_snapshot` or a ledger (`ReferenceScriptState` holds only the `onlyOnce` trigger parked bit) carries them.

Trigger: any game; a recognized `default2StateActivator` (Skyrim levers, gates, puzzle doors) opened by the player, or any Oblivion/FO3/FNV `SCRI` object script that keeps a local (`short bDone`, a timer), then leave the cell (interior door, or the tile leaves the streaming radius) and come back.

## Evidence
`two_state_activator.rs:34-42` builds `TwoStateActivator { is_open: <authored>, activated_once: false, .. }` on every attach. `obscript_objects.rs:204-208` is `if !world.has::<ScriptVariables>(entity) { world.insert(entity, ScriptVariables::default()); }`. `attach_script_for_refr` (`attach.rs:152-176`) then emits `OnCellLoadEvent`, which `obscript_objects.rs:263` turns into an `OnLoad` run on every attach. The module doc at `obscript_objects.rs:51-54` states the locals "survive a cell unload and a save without a second ledger"; only the "save" half is true. `docs/engine/stream-boundary-state-continuity.md:133-136` ("Script-local Papyrus VM state - canonical save/load already owns this boundary") has the same blind spot: the save registry owns resident entities, not evicted ones.

## Impact
An opened gate/lever/puzzle door closes again after the player leaves and returns; `do_once` / `activated_once` gating re-arms. For ObScript object scripts, which are default-on (`BYRO_NO_OBSCRIPT_OBJECTS` is the only switch), a script that guards an `OnLoad` side effect with a local flag runs that effect again on each re-entry while the reference's *inventory* correctly persists (it is in `ReferenceState`), so the effect can duplicate items. The counts of vanilla scripts that depend on this were not measured (no game data read).

## Related
#2291 (closed, resident-save columns only), #4334 / `ReferenceScriptState` (the existing per-reference ledger shape), #6017 (provider `OnInit` once-only flag, same class), GAME-SD-02, GAME-SD-03, SAVE-SD-03 (further columns missing from the same row). Prior pointer: `AUDIT_GAMEPLAY_2026-09-21.md:697` (no issue was filed).

## Suggested Fix
Add a `script_state: Option<(TwoStateActivator, ScriptVariables)>` pair to `ReferenceState` (a `FORMAT_MAJOR` bump; no serde defaults, #4465) captured for any victim carrying either, consumed by `restore` *after* the recognizer/`attach_object_script` stamps (so the authored defaults are overwritten). Correct the two doc claims. Pin with a test that evicts an opened `TwoStateActivator` and a scripted local, respawns, and asserts both survive.

Source: docs/audits/AUDIT_GAMEPLAY_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other per-placement components absent from the eviction row)
- [ ] **TESTS**: A regression test pins this specific fix
