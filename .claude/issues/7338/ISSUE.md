# #7338: GP-02: A scripted door's `::isOpen_var` is still parked and restored while its `TwoStateActivator` is not (#7305 left the half)

- **Severity**: MEDIUM
- **Dimension**: 2 (Interaction, doors, reference_state persistence)
- **Location**: `byroredux/src/cell_loader/reference_state.rs:523-543` (`capture_script_states`), restore at `:560-587` (`restore_script_state`); stamp at `crates/scripting/src/translate/recognizers/two_state_activator.rs:42-48`
- **Status**: NEW (sibling of #7305 / 7a7cc9aa7, closed)
- **Trigger**: a placed DOOR whose model authors `Open`/`Close` sequences (so it gets a `Door`) and whose base or REFR carries a Papyrus script recognised as `default2StateActivator` (Skyrim+ script on a door with NiControllerSequences). The player opens the door, leaves the cell (eviction), returns in the same session. Not reachable through a save-load (the `TwoStateActivator` and `ScriptVariables` snapshot columns are both saved and re-applied together).
- **Description**: #7305 made `capture_script_states` skip `TwoStateActivator` for an entity carrying `Door`, so the door returns to its `Open by Default` state on re-entry (DRT-STATE-003). The same function still parks `ScriptVariables` unconditionally. The recognizer's variable set is `{::isOpen_var, ::isAnimating_var, ::doOnce_var}`, so a scripted door always has `variables: Some(..)`, its row survives `states.retain(!is_empty)`, and `restore_script_state` writes the old `::isOpen_var` / `::isAnimating_var` back over the fresh values the recognizer just stamped from the default state. After re-entry the door is at its default pose and `TwoStateActivator.is_open` is the default, while `::isOpen_var` still holds what it was when the player left.
- **Evidence**:
  ```rust
  // reference_state.rs (#7305): only the two-state half is skipped for a Door
  for (((entity, _, _), (_, state)), door) in rows.iter().zip(&mut states).zip(&doors) {
      if !door { state.two_state = two_state_q.get(*entity).copied(); }
  }
  // the next block, unconditional:
  state.variables = variables_q.get(*entity).cloned();
  // two_state_activator.rs:45-47, the variables that mirror the state
  variables.set_by_name("::isOpen_var", f32::from(is_open));
  variables.set_by_name("::isAnimating_var", ...); variables.set_by_name("::doOnce_var", ...);
  // restore_script_state, over the fresh stamp
  if let Some(variables) = state.variables { world.insert(entity, variables); }
  ```
  `::isOpen_var` is the CTDA-visible mirror (`vm_state.rs` `IS_OPEN_VARIABLE`, rewritten on every toggle and on `SetOpen`; read by `GetVMScriptVariable` conditions, `condition/tests.rs:602`). The guard `a_doors_open_state_is_not_parked_but_a_levers_is` puts no `ScriptVariables` on its door, so it cannot see this.
- **Impact**: any condition (quest, package, scene, dialogue) that reads `::isOpen_var` of such a door sees the stale open/closed answer until the next toggle, while the door itself is at default. Narrow reach today (Skyrim+ doors that author sequences, with the recognised script), hence MEDIUM not HIGH.
- **Related**: #7305, #7306, #6071.
- **Suggested Fix**: for a `Door` entity, also reset the three two-state-mirror names in the parked `ScriptVariables` (or skip `variables` when those are its only entries), or re-derive the mirror from the surviving `TwoStateActivator` after `restore_script_state`. Extend the guard with a `ScriptVariables` row on the door.

---
- **Dimension agent ID**: GP-D2-01

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other action systems / movers / park paths / add paths named in the finding)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved and the system's `Access` row matches
- [ ] **TESTS**: A regression test pins this specific fix

**Found by**: `docs/audits/AUDIT_GAMEPLAY_2026-11-23.md`
