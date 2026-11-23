# #7346: SAVE-01: #7342 consumes a ring door's saved TwoStateActivator before the door script's recognizer, which re-stamps the whole component

- **Severity**: MEDIUM
- **Dimension**: Live Load-Apply
- **Data-Loss Class**: silent-drop
- **Location**: `byroredux/src/systems/door.rs:193-225` (`attach_door` → `restore_parked_door_state`), `byroredux/src/cell_loader/reference_state.rs:596-625`, vs `crates/scripting/src/translate/recognizers/two_state_activator.rs:36-48`; call order `cell_loader/references/synth_child.rs:920` (`attach_door`) before `:983` (`attach_script_for_refr`)
- **Status**: NEW (regression of the scripted half of #7342 / GP-06 relative to the pre-fix path; related #6071)
- **Description**: A save's unresolved references (the exterior hysteresis ring) are parked with their saved `TwoStateActivator` in `scripts[pair].two_state`. Before #7342 a scripted door got it back in `restore_script_state`, which runs after the script attach and overwrites the recognizer's stamp. #7342 moved the consumption into `attach_door`, which runs BEFORE `attach_script_for_refr`. For a door carrying `default2StateActivator` the recognizer then replaces the component wholesale:
  ```rust
  // two_state_activator.rs:39-43
  let is_open = authored_open.unwrap_or_else(|| world.get::<TwoStateActivator>(entity).map_or(default_open, |seeded| seeded.is_open));
  let state = TwoStateActivator { is_open, is_animating, do_once, activated_once: false };
  world.insert(entity, state);
  ```
  So the restored `activated_once` is always reset to `false`, `is_animating` and `do_once` come from the script defaults, and `is_open` comes from the placement's VMAD `isOpen` when one is authored (`authored_open`), not from the save. `restore_parked_door_state` already took `two_state` out of the row (`state.two_state.take()`), so `restore_script_state` later has nothing to put back (it restores only `variables`, mirrored from the already-overwritten component). The test `a_saved_door_state_parked_for_the_ring_is_put_on_the_door_at_attach` runs `scripted: true` but only through `attach_door`; the recognizer never runs in it.
- **Trigger**: exterior save, a Skyrim+/FO4 door with `default2StateActivator` in the ring (`radius_load + 1`, not respawned by the reload), already used. The streamed-in door has `activated_once = false`: a `doOnce` door can be used again (`two_state_activator_system` skips only `do_once && activated_once`, `vm_state.rs:170`); a door whose VMAD authors `isOpen` returns to the authored state instead of the saved one. A resident door is unaffected (the `TwoStateActivator` column overlays after the reload).
- **Impact**: narrow, but it is saved gameplay state silently replaced by authored values, which DRT-STATE-003 / DRT-SAVE-001 say must be kept ("a cell restored from a save MUST keep the saved state"). The fix for the unscripted door left the scripted one worse than before it.
- **Related**: #7342 (closed), #7338, #7305, #6071. Searched "door parked state scripted TwoStateActivator recognizer" and "activated_once door save": only the closed issues above.
- **Suggested Fix**: in `restore_parked_door_state` leave `two_state` in the row when the row also holds `variables` / `timer` / `rumble` (a script will attach and `restore_script_state` puts it back after the recognizer, then re-derives the mirror), and consume it only for a row that holds nothing else. Extend the test to run the recognizer between `attach_door` and `restore_script_state`.
- **Dimension agent ID**: SAVE-D5-02

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other parked-state paths, load-drain steps, notification callers and `atomic_write` callers the finding names)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix

**Found by**: `docs/audits/AUDIT_SAVE_2026-11-23.md`
